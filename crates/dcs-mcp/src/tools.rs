//! The six tools, in one place.
//!
//! One impl block holds all six, and the server's listing is taken off the
//! router that block builds. That is the whole reason for the arrangement:
//! the set that is registered is the set that is listed, because there is no
//! second list for one of them to fall out of. A tool added to this block is
//! listed by having been added; a tool that would be registered and not
//! listed has nowhere to be written.
//!
//! Every body blocks rather than awaits. The work behind a call is a handful
//! of small local file reads and, for the three that publish, a wait over a
//! directory — the same choice the serve module already argues for its own
//! reads, and the reason the server runs on one current-thread runtime.
//!
//! **How a reply is worded is not settled here.** Every answer goes out
//! through the one renderer in `wording`, which is what lets the wording
//! change in one function rather than in six bodies.
//!
//! Each body is one line over a function further down, and those functions
//! are what the command line calls too. Two callers over one function is the
//! only arrangement in which the words a tool gives and the words a terminal
//! prints cannot come apart.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use dcs_eval::file::{self, Roots};
use dcs_eval::game;
use dcs_eval::paths::{self, Real};
use dcs_eval::pipeline::{Pipeline, Spec};
use dcs_eval::reads::Tiers;
use dcs_eval::wait::{self, Collected, Outcome};
use dcs_eval::{source, status};
use rmcp::ErrorData;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, ContentBlock};
use rmcp::{tool, tool_router};

use crate::register::{DataDir, RegisterError};
use crate::serve::{Client, Host, Serve, host_of, output_in};
use crate::verify;
use crate::wording::{self, answered, pending, refuse, say};

/// How long a call waits on a reply before answering `pending`.
///
/// It is a wait and never a limit. Nothing is cancelled when it runs out: the
/// request stays published, the answer names an id, and the reply is picked
/// up later. A mission load on its own can outlast this several times over,
/// so a caller that waited longer has not failed at anything.
const DEFAULT_WAIT: Duration = Duration::from_secs(15);

/// The wait this call asked for, or the default where it asked for none.
pub(crate) fn waiting(seconds: Option<u64>) -> Duration {
    seconds.map_or(DEFAULT_WAIT, Duration::from_secs)
}

/// The host this call is about: the one its `host` argument names, or the
/// one the `--host` flag named where it names none.
///
/// Apart from finding the executor, because a call can want the host without
/// wanting a session. Resolving a client reads `executor.txt`, which is not
/// there precisely when nothing is installed — and a report about the
/// install has to be able to answer in that case rather than refuse before
/// it has looked.
fn host_for(serve: &Serve, host: Option<&str>) -> Result<Host, CallToolResult> {
    match host {
        Some(word) => host_of(word).ok_or_else(|| {
            refuse(
                "bad-argument",
                vec![format!("host is hook or export, not {word}")],
            )
        }),
        None => Ok(serve.options().host),
    }
}

/// The executor this call is about: the session published for that host.
fn client_for(serve: &Serve, host: Option<&str>) -> Result<Client, CallToolResult> {
    let host = host_for(serve, host)?;
    serve
        .client_at(host)
        .map_err(|why| refuse("no-session", vec![why.to_string()]))
}

/// The write directories the containment rules fire on: the one variant this
/// server was pointed at.
///
/// Dropped where it will not resolve, rather than refused. A directory that
/// is not on the disk contains nothing, and with no root allowed every path
/// is refused anyway — so refusing the call outright would replace an answer
/// about the path with an answer about the install.
pub(crate) fn writedirs(serve: &Serve) -> Vec<Real> {
    let opts = serve.options();
    paths::resolve(&opts.saved_games.join(&opts.variant))
        .into_iter()
        .collect()
}

/// Where this build's own files go for this server: the directory the
/// options name, or the known folder where they name none.
///
/// One resolver, because the run record and a captured reply must land in
/// the same place — two of these would let `--data-dir` move one of them and
/// not the other. The DCS write directory is passed as the tree the data
/// directory may not lie under either way, so the containment rule the
/// install paths are judged by is the one every file here is judged by.
pub(crate) fn data_dir(serve: &Serve) -> Result<DataDir, RegisterError> {
    let dirs = writedirs(serve);
    let trees: Vec<&Real> = dirs.iter().collect();
    match &serve.options().data_dir {
        Some(path) => DataDir::at(path, &trees),
        None => DataDir::known(&trees),
    }
}

/// One reply, as the executor published it: the id it came back under and
/// the bytes that were on the disk.
pub(crate) struct Reply {
    pub id: String,
    pub bytes: Vec<u8>,
}

/// An answer, and — where exactly one reply came off the wire — the id it
/// came back under and the file the executor published it in.
///
/// `at` is `None` where there is no single reply to point at: a `pending`, a
/// refusal raised here rather than by the executor, and the two calls that
/// never publish anything.
pub(crate) struct Answered {
    pub answer: CallToolResult,
    at: Option<(String, PathBuf)>,
}

impl Answered {
    /// An answer with no reply behind it.
    fn plain(answer: CallToolResult) -> Self {
        Self { answer, at: None }
    }

    /// The bytes the executor published for this answer, read off the disk
    /// when they are asked for rather than when the answer was made.
    ///
    /// Nothing where no single reply came off the wire, and `Some(Err)` where
    /// one did and its file could not be read back. The read waits for the
    /// asking because only a command line told to keep the reply ever wants
    /// the bytes: a tool call reads the answer and nothing else, and reading
    /// every body eagerly would cost each of those calls a copy of a reply
    /// nobody looks at.
    pub(crate) fn published(&self) -> Option<Result<Reply, String>> {
        let (id, path) = self.at.as_ref()?;
        Some(
            fs::read(path)
                .map(|bytes| Reply {
                    id: id.clone(),
                    bytes,
                })
                .map_err(|why| format!("{}: {why}", path.display())),
        )
    }
}

/// Publish one request over the session and render what that one came to.
fn one(client: &Client, spec: Spec, upto: Duration) -> Answered {
    let mut window = Pipeline::over(client.handshake(), vec![spec], 1, upto);
    let item = window.next();
    // The file, not the envelope. The envelope is a parse, and parsing
    // normalises a header line's ending; what a caller asking to keep the
    // reply is asking for is the bytes the executor wrote, which is the file
    // and not a second encoding of it.
    let at = match &item {
        Some(Ok(Outcome::Reply(envelope))) => {
            let id = envelope.headers.get("id").unwrap_or_default().to_owned();
            let path = client.session().res().join(format!("{id}.res"));
            Some((id, path))
        }
        _ => None,
    };
    Answered {
        answer: answered(item),
        at,
    }
}

/// `dcs_status`'s arguments.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct Status {
    /// `hook` or `export`. The server's own `--host` decides where this is
    /// left out.
    pub host: Option<String>,
}

/// `dcs_ping`'s arguments.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct Ping {
    /// `hook` or `export`.
    pub host: Option<String>,
    /// How long to wait for the reply. A wait, never a limit: when it runs
    /// out the answer is a `pending` naming an id to collect.
    pub wait_seconds: Option<u64>,
}

/// `dcs_game_state`'s arguments.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct GameState {
    /// `hook` or `export`.
    pub host: Option<String>,
    /// How long to wait on the window of reads.
    pub wait_seconds: Option<u64>,
}

/// `dcs_eval`'s arguments.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct Eval {
    /// `hook` or `export`.
    pub host: Option<String>,
    /// Which Lua state the chunk runs in.
    pub state: String,
    /// The chunk itself.
    pub code: String,
    /// The name the chunk is compiled under, so a raise names something the
    /// caller recognises.
    pub chunkname: Option<String>,
    /// The instruction budget the chunk is bounded by.
    pub max_instructions: Option<u64>,
    /// How long to wait for the reply.
    pub wait_seconds: Option<u64>,
}

/// `dcs_eval_file`'s arguments.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct EvalFile {
    /// `hook` or `export`.
    pub host: Option<String>,
    /// Which Lua state the chunk runs in.
    pub state: String,
    /// The file the chunk is read from. It is read by this server and not by
    /// DCS, and the path is judged before anything opens it.
    pub path: String,
    /// The instruction budget the chunk is bounded by.
    pub max_instructions: Option<u64>,
    /// How long to wait for the reply.
    pub wait_seconds: Option<u64>,
}

/// `dcs_collect`'s arguments.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct Collect {
    /// `hook` or `export`.
    pub host: Option<String>,
    /// The id a `pending` answer named.
    pub id: String,
}

/// What `dcs_status` and the `status` verb both do.
///
/// No client is resolved, deliberately. Resolving one reads the handshake,
/// which is absent exactly when nothing is installed — so a call that
/// resolved first would refuse to say anything about the install in the one
/// case a user most needs it to.
pub(crate) fn status(serve: &Serve, host: Option<&str>) -> Answered {
    let host = match host_for(serve, host) {
        Ok(host) => host,
        Err(no) => return Answered::plain(no),
    };
    // The session in full sits under the report, rendered through `Debug` on
    // purpose and for now: the summary above is what a reader needs first,
    // and a second wording of the rest would be inventing one twice.
    Answered::plain(say(
        "status",
        match writedirs(serve).into_iter().next() {
            Some(variant) => {
                // Under the variant as it resolved, so the report does not
                // name one path resolved and its neighbour as the flags
                // spelt it.
                let output = output_in(variant.as_path(), host);
                let report = verify::verify(&variant, &output);
                let session = format!("{:#?}", report.session);
                vec![report.to_string(), session]
            }
            // The install is not on the disk, so there is nothing of it to
            // look at — and the session half is still printed, since a
            // directory that will not resolve says nothing about a handshake
            // that might.
            None => vec![
                format!(
                    "{}: the install could not be looked at, so only the session is reported",
                    serve
                        .options()
                        .saved_games
                        .join(&serve.options().variant)
                        .display()
                ),
                format!(
                    "{:#?}",
                    status::status(&serve.options().at_host(host).output())
                ),
            ],
        },
    ))
}

/// What `dcs_ping` and the `ping` verb both do.
pub(crate) fn ping(serve: &Serve, host: Option<&str>, upto: Duration) -> Answered {
    let client = match client_for(serve, host) {
        Ok(client) => client,
        Err(no) => return Answered::plain(no),
    };
    let stamp = client.handshake().stamp.clone();
    let spec = Spec::new(&[("op", "ping"), ("for", &stamp)], b"");
    one(&client, spec, upto)
}

/// What `dcs_game_state` and the `game-state` verb both do.
pub(crate) fn game_state(serve: &Serve, host: Option<&str>, upto: Duration) -> Answered {
    let client = match client_for(serve, host) {
        Ok(client) => client,
        Err(no) => return Answered::plain(no),
    };
    Answered::plain(
        match game::game_state(client.output().as_path(), Tiers::default(), upto) {
            Ok(state) => say("game-state", vec![state.to_string()]),
            Err(why) => refuse("refused", vec![why.to_string()]),
        },
    )
}

/// What `dcs_eval` and the `eval` verb both do.
pub(crate) fn eval(
    serve: &Serve,
    host: Option<&str>,
    state: &str,
    code: &str,
    chunkname: Option<&str>,
    max_instructions: Option<u64>,
    upto: Duration,
) -> Answered {
    let client = match client_for(serve, host) {
        Ok(client) => client,
        Err(no) => return Answered::plain(no),
    };
    let stamp = client.handshake().stamp.clone();
    let budget = max_instructions.map(|max| max.to_string());
    let mut headers = vec![("op", "eval"), ("for", stamp.as_str()), ("state", state)];
    if let Some(name) = chunkname {
        headers.push(("chunkname", name));
    }
    if let Some(budget) = &budget {
        headers.push(("max_instructions", budget.as_str()));
    }
    let spec = Spec::new(&headers, code.as_bytes());
    one(&client, spec, upto)
}

/// What `dcs_eval_file` and the `eval --file` verb both do.
pub(crate) fn eval_file(
    serve: &Serve,
    host: Option<&str>,
    state: &str,
    path: &str,
    max_instructions: Option<u64>,
    upto: Duration,
) -> Answered {
    let client = match client_for(serve, host) {
        Ok(client) => client,
        Err(no) => return Answered::plain(no),
    };
    let refused = |why: String| Answered::plain(refuse("refused", vec![why]));
    let real = match paths::resolve(Path::new(path)) {
        Ok(real) => real,
        Err(why) => return refused(why.to_string()),
    };
    // No root is allowed, because nothing configures one yet, and an empty
    // allowed list admits nothing rather than everything. Every path is
    // refused here until a root can be named.
    let roots = match Roots::new(&[], &writedirs(serve), None) {
        Ok(roots) => roots,
        Err(why) => return refused(why.to_string()),
    };
    let chunkname = match source::chunkname(&real) {
        Ok(name) => name,
        Err(why) => return refused(why.to_string()),
    };
    let stamp = client.handshake().stamp.clone();
    let budget = max_instructions.map(|max| max.to_string());
    let mut headers = vec![
        ("op", "eval"),
        ("for", stamp.as_str()),
        ("state", state),
        ("chunkname", chunkname.as_str()),
    ];
    if let Some(budget) = &budget {
        headers.push(("max_instructions", budget.as_str()));
    }
    // The same header slice measures the file and frames the request, so the
    // ceiling the file was admitted under is the one the request is really
    // written against.
    let admitted = match file::check(&roots, client.handshake(), &headers, &real) {
        Ok(admitted) => admitted,
        Err(why) => return refused(why.to_string()),
    };
    let source = match source::read(&admitted) {
        Ok(source) => source,
        Err(why) => return refused(why.to_string()),
    };
    let spec = Spec::new(&headers, source.body());
    let mut answered = one(&client, spec, upto);
    // Which bytes ran, said beside the answer rather than instead of it.
    answered.answer.content.push(ContentBlock::text(format!(
        "source: {}\nsha256: {}",
        source.path(),
        source.sha256_hex()
    )));
    answered
}

#[tool_router(vis = "pub(crate)")]
impl Serve {
    /// What is readable without asking the executor anything: the hook's
    /// hash, the line in `Export.lua`, any second hook beside ours, the two
    /// policy-gate keys, the session it published, whether that process is
    /// alive, how old the heartbeat is, and every problem found along the
    /// way.
    #[tool]
    async fn dcs_status(
        &self,
        Parameters(args): Parameters<Status>,
    ) -> Result<CallToolResult, ErrorData> {
        Ok(status(self, args.host.as_deref()).answer)
    }

    /// Prove the executor is alive by getting a reply out of it.
    #[tool]
    async fn dcs_ping(
        &self,
        Parameters(args): Parameters<Ping>,
    ) -> Result<CallToolResult, ErrorData> {
        Ok(ping(self, args.host.as_deref(), waiting(args.wait_seconds)).answer)
    }

    /// What the game is doing, every fact it rests on, and the basis of each
    /// value.
    #[tool]
    async fn dcs_game_state(
        &self,
        Parameters(args): Parameters<GameState>,
    ) -> Result<CallToolResult, ErrorData> {
        Ok(game_state(self, args.host.as_deref(), waiting(args.wait_seconds)).answer)
    }

    /// Run a chunk of Lua inside the running game and report what it came to.
    #[tool]
    async fn dcs_eval(
        &self,
        Parameters(args): Parameters<Eval>,
    ) -> Result<CallToolResult, ErrorData> {
        Ok(eval(
            self,
            args.host.as_deref(),
            &args.state,
            &args.code,
            args.chunkname.as_deref(),
            args.max_instructions,
            waiting(args.wait_seconds),
        )
        .answer)
    }

    /// The same, over a file this server reads. The path is judged before any
    /// byte of it is opened, and the answer carries where the bytes came from
    /// and what they hashed to.
    #[tool]
    async fn dcs_eval_file(
        &self,
        Parameters(args): Parameters<EvalFile>,
    ) -> Result<CallToolResult, ErrorData> {
        Ok(eval_file(
            self,
            args.host.as_deref(),
            &args.state,
            &args.path,
            args.max_instructions,
            waiting(args.wait_seconds),
        )
        .answer)
    }

    /// Pick up a reply a `pending` answer left behind. One look, no waiting,
    /// and the request is not sent again.
    #[tool]
    async fn dcs_collect(
        &self,
        Parameters(args): Parameters<Collect>,
    ) -> Result<CallToolResult, ErrorData> {
        let client = match client_for(self, args.host.as_deref()) {
            Ok(client) => client,
            Err(no) => return Ok(no),
        };
        Ok(match wait::collect(client.session(), &args.id) {
            // Through the same renderer a waited-for reply goes through: a
            // reply that refused is a refusal whether it was waited on or
            // picked up afterwards.
            Ok(Collected::Reply(envelope)) => wording::reply(&envelope),
            Ok(Collected::Foreign { saw, wanted }) => refuse(
                "stale-session",
                vec![format!(
                    "the reply under {} is stamped {saw}, and the session addressed is {wanted}",
                    args.id
                )],
            ),
            // Nothing has landed, and one look says nothing about why, so
            // the phase is the one the wait carries when the disk says
            // nothing either — named rather than left out, because a caller
            // deciding whether to look again needs the same two facts here
            // as in the `pending` that sent it.
            Ok(Collected::Nothing) => pending(
                &args.id,
                wait::PHASE_UNKNOWN,
                None,
                Some("nothing has landed under that id yet"),
            ),
            Err(why) => refuse("refused", vec![why.to_string()]),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::serve::{Host, Options};
    use crate::testing::Sandbox;
    use dcs_eval::standin::Standin;
    use rmcp::ServiceExt;
    use rmcp::model::CallToolRequestParams;
    use rmcp::service::{RoleClient, RoleServer, RunningService};
    use std::fs;

    /// The six, spelt once and sorted, which is what the listing is compared
    /// against.
    const SIX: [&str; 6] = [
        "dcs_collect",
        "dcs_eval",
        "dcs_eval_file",
        "dcs_game_state",
        "dcs_ping",
        "dcs_status",
    ];

    /// A server and a client joined over an in-memory pair, with a stand-in
    /// executor already published where the server will look.
    ///
    /// The two halves are started together rather than one after the other:
    /// the client's own `serve` drives `initialize` and cannot finish until
    /// the server has answered it, so awaiting either on its own deadlocks.
    async fn pair(
        box_: &Sandbox,
    ) -> (
        RunningService<RoleServer, Serve>,
        RunningService<RoleClient, ()>,
    ) {
        let opts = Options {
            saved_games: box_.path.clone(),
            variant: "DCS.openbeta".to_owned(),
            host: Host::Hook,
            // Inside the box: every evaluation writes a run record, and a
            // fixture left pointing at the known folder would write it into
            // the machine's own data directory.
            data_dir: Some(box_.join("data")),
        };
        let mut ex = Standin::open(&opts.output(), "hook").expect("the stand-in opens");
        // The stand-in's own pid is a number nobody is running, and a session
        // whose process is gone answers `Dead` rather than `Pending`. Both are
        // valid answers to these calls; the live one is the interesting one.
        ex.pid = std::process::id();
        ex.handshake().expect("the handshake is published");

        // Roomier than the SDK's own tests use, because six tool schemas go
        // down this in one frame and a writer that stalled here would look
        // from the far side like a server that hung.
        let (server_io, client_io) = tokio::io::duplex(64 * 1024);
        let (server, client) = tokio::join!(Serve::new(opts).serve(server_io), ().serve(client_io));
        (
            server.expect("the server side comes up"),
            client.expect("the client side comes up"),
        )
    }

    /// The row this work is proved by. Not "six calls succeeded" — a tool can
    /// be registered and reachable while the listing leaves it out, and a
    /// client that cannot see it will never call it. So the listed set is
    /// compared whole, which catches a misspelt name, a seventh tool and a
    /// duplicate alike, and prints both lists when it fails.
    #[tokio::test]
    async fn tools_listed_are_exactly_the_six() {
        let box_ = Sandbox::new();
        let (server, client) = pair(&box_).await;

        let mut listed: Vec<String> = client
            .list_tools(None)
            .await
            .expect("the listing is answered")
            .tools
            .into_iter()
            .map(|tool| tool.name.to_string())
            .collect();
        listed.sort();
        assert_eq!(listed, SIX, "the listed set is the six and only the six");

        client.cancel().await.expect("the client hangs up");
        server.cancel().await.expect("the server comes down");
    }

    /// Each listed name reaches a body. A tool listed but not routed answers
    /// with a protocol error saying the tool was not found, which is what the
    /// six `Ok`s here rule out — and the seventh call, of a name that really
    /// is not there, is what says those six are not vacuous.
    ///
    /// It goes no further than that, on purpose. What a body *says* is not
    /// read here, so a handler answering the wrong question in a well-formed
    /// block would still pass. The wording of a reply is settled in one
    /// place, by the tests that own it, and not by this one.
    #[tokio::test]
    async fn tools_listed_each_answer_a_call() {
        let box_ = Sandbox::new();
        let chunk = box_.join("chunk.lua");
        fs::write(&chunk, b"return 1\n").expect("the chunk is written");
        let (server, client) = pair(&box_).await;

        let arguments = |json: serde_json::Value| match json {
            serde_json::Value::Object(map) => map,
            other => panic!("the arguments are an object: {other}"),
        };
        // Every wait is zero, because the stand-in answers nothing until it
        // is ticked: a call that waited would hold the one runtime the client
        // is waiting on.
        let calls = [
            ("dcs_status", serde_json::json!({})),
            ("dcs_ping", serde_json::json!({ "wait_seconds": 0 })),
            ("dcs_game_state", serde_json::json!({ "wait_seconds": 0 })),
            (
                "dcs_eval",
                serde_json::json!({ "state": "hook", "code": "return 1", "wait_seconds": 0 }),
            ),
            (
                "dcs_eval_file",
                serde_json::json!({
                    "state": "hook",
                    "path": chunk.to_string_lossy(),
                    "wait_seconds": 0,
                }),
            ),
            (
                "dcs_collect",
                serde_json::json!({ "id": "0000000001-abcd1234" }),
            ),
        ];
        for (name, args) in calls {
            let answer = client
                .call_tool(CallToolRequestParams::new(name).with_arguments(arguments(args)))
                .await
                .unwrap_or_else(|why| panic!("{name} is routed and answers: {why}"));
            // Not whether it refused. With no root allowed and no game
            // running, a refusal is the correct answer to several of these;
            // what is asserted is that something came back to say so.
            assert!(
                !answer.content.is_empty(),
                "{name} answered with no content at all"
            );
        }

        let missing = client
            .call_tool(CallToolRequestParams::new("dcs_nope"))
            .await;
        assert!(
            missing.is_err(),
            "a name that is not registered is refused, so the six above mean something"
        );

        client.cancel().await.expect("the client hangs up");
        server.cancel().await.expect("the server comes down");
    }

    /// The one answer whose wording is decided here rather than in the
    /// renderer: a collect that found nothing chooses the phase it reports,
    /// because one look at the disk says nothing about which one the session
    /// is in. The renderer cannot be held to a choice its caller makes, so
    /// this drives the body over the wire and reads what came back — and its
    /// name carries `wording` so the filtered command that owns that question
    /// selects it along with the rest.
    ///
    /// Not an error, and that half matters as much: a caller branching on the
    /// error flag must be told to look again, not to give up.
    #[tokio::test]
    async fn tools_listed_wording_of_an_uncollected_id_names_a_phase() {
        let box_ = Sandbox::new();
        let (server, client) = pair(&box_).await;

        let mut arguments = serde_json::Map::new();
        arguments.insert(
            "id".to_owned(),
            serde_json::Value::String("0000000001-abcd1234".to_owned()),
        );
        let answer = client
            .call_tool(CallToolRequestParams::new("dcs_collect").with_arguments(arguments))
            .await
            .expect("dcs_collect answers");

        let rendered = answer
            .content
            .iter()
            .filter_map(|block| block.as_text().map(|text| text.text.as_str()))
            .collect::<Vec<_>>()
            .join("\n");
        assert_ne!(
            answer.is_error,
            Some(true),
            "nothing failed, so the answer is not an error: {rendered}"
        );
        assert_eq!(
            rendered.lines().next().unwrap_or_default(),
            "pending",
            "it is headed `pending`: {rendered}"
        );
        assert!(
            rendered.contains("id: 0000000001-abcd1234"),
            "it names the id to collect under: {rendered}"
        );
        assert!(
            rendered.contains("phase: "),
            "it names a phase rather than leaving one out: {rendered}"
        );

        client.cancel().await.expect("the client hangs up");
        server.cancel().await.expect("the server comes down");
    }

    /// The install half of `dcs_status`, over a sandbox that has no
    /// `Scripts\Hooks` in it at all.
    ///
    /// Two things at once, and the second is the one that would go unnoticed:
    /// that the answer names the hook that is not there, and that there is an
    /// answer at all. A body that resolved a client first would refuse here —
    /// the handshake is published, but the directory holding the executor is
    /// not, which is what "nothing is installed" looks like — and a refusal
    /// carries content too, so the call that merely checks for content cannot
    /// tell the two apart.
    #[tokio::test]
    async fn tools_listed_verify_names_the_hook_that_is_not_there() {
        let box_ = Sandbox::new();
        let (server, client) = pair(&box_).await;

        let answer = client
            .call_tool(CallToolRequestParams::new("dcs_status"))
            .await
            .expect("dcs_status answers");

        let rendered = answer
            .content
            .iter()
            .filter_map(|block| block.as_text().map(|text| text.text.as_str()))
            .collect::<Vec<_>>()
            .join("\n");
        assert_ne!(
            answer.is_error,
            Some(true),
            "a report is not a refusal: {rendered}"
        );
        assert!(
            rendered.contains("hook: not there"),
            "the install half is reported: {rendered}"
        );
        assert!(
            rendered.contains("DcsEvalExecutor.lua"),
            "and it names the file that is missing: {rendered}"
        );

        client.cancel().await.expect("the client hangs up");
        server.cancel().await.expect("the server comes down");
    }

    /// The claim made on the wire, read off the client's own copy of what
    /// `initialize` answered. A server holding six tools and announcing no
    /// tool capability is one a client never asks for a listing.
    #[tokio::test]
    async fn tools_listed_are_announced_as_a_capability() {
        let box_ = Sandbox::new();
        let (server, client) = pair(&box_).await;

        let info = client.peer_info().expect("the server answered initialize");
        assert!(
            info.capabilities.tools.is_some(),
            "the server announces the tool capability: {:?}",
            info.capabilities
        );

        client.cancel().await.expect("the client hangs up");
        server.cancel().await.expect("the server comes down");
    }

    /// The naming rule, held rather than described.
    ///
    /// The filtered command this work is proved by selects by substring, so a
    /// test here under another name is neither run by it nor missed by it. A
    /// rule written only in a comment is one a later test is added in breach
    /// of, so this reads the source it is written in and holds it.
    #[test]
    fn tools_listed_names_every_test_here() {
        let source = include_str!("tools.rs");
        let mut lines = source.lines().enumerate();
        while let Some((number, line)) = lines.next() {
            let marker = line.trim();
            if marker != concat!("#[", "test]") && marker != concat!("#[", "tokio::test]") {
                continue;
            }
            let declared = lines
                .by_ref()
                .map(|(_, next)| next.trim_start())
                .find(|next| next.contains("fn "))
                .unwrap_or_else(|| {
                    panic!("the test marker on line {} declares nothing", number + 1)
                });
            let name = declared
                .split("fn ")
                .nth(1)
                .and_then(|rest| rest.split(['(', '<']).next())
                .expect("the declaration names the function");
            assert!(
                name.starts_with("tools_listed_"),
                "the test marked on line {} is named `{name}`, which the filtered \
                 command this file is proved by would not select",
                number + 1
            );
        }
    }
}
