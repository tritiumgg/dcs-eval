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

use std::path::Path;
use std::time::Duration;

use dcs_eval::file::{self, Roots};
use dcs_eval::game;
use dcs_eval::paths::{self, Real};
use dcs_eval::pipeline::{Pipeline, Spec};
use dcs_eval::reads::Tiers;
use dcs_eval::wait::{self, Collected};
use dcs_eval::{source, status};
use rmcp::ErrorData;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{CallToolResult, ContentBlock};
use rmcp::{tool, tool_router};

use crate::serve::{Client, Serve, host_of};
use crate::wording::{answered, refuse, reply_lines, say};

/// How long a call waits on a reply before answering `pending`.
///
/// It is a wait and never a limit. Nothing is cancelled when it runs out: the
/// request stays published, the answer names an id, and the reply is picked
/// up later. A mission load on its own can outlast this several times over,
/// so a caller that waited longer has not failed at anything.
const DEFAULT_WAIT: Duration = Duration::from_secs(15);

/// The wait this call asked for, or the default where it asked for none.
fn waiting(seconds: Option<u64>) -> Duration {
    seconds.map_or(DEFAULT_WAIT, Duration::from_secs)
}

/// The executor this call is about: the host its `host` argument names, or
/// the one the `--host` flag named where it names none.
fn client_for(serve: &Serve, host: Option<&str>) -> Result<Client, CallToolResult> {
    let host = match host {
        Some(word) => host_of(word).ok_or_else(|| {
            refuse(
                "bad-argument",
                vec![format!("host is hook or export, not {word}")],
            )
        })?,
        None => serve.options().host,
    };
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
fn writedirs(serve: &Serve) -> Vec<Real> {
    let opts = serve.options();
    paths::resolve(&opts.saved_games.join(&opts.variant))
        .into_iter()
        .collect()
}

/// Publish one request over the session and render what that one came to.
fn one(client: &Client, spec: Spec, upto: Duration) -> CallToolResult {
    let mut window = Pipeline::over(client.handshake(), vec![spec], 1, upto);
    answered(window.next())
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

#[tool_router(vis = "pub(crate)")]
impl Serve {
    /// What is readable without asking the executor anything: whether it is
    /// installed, the session it published, whether that process is alive,
    /// whether it is armed, how old the heartbeat is, and every problem found
    /// along the way.
    #[tool]
    async fn dcs_status(
        &self,
        Parameters(args): Parameters<Status>,
    ) -> Result<CallToolResult, ErrorData> {
        let client = match client_for(self, args.host.as_deref()) {
            Ok(client) => client,
            Err(no) => return Ok(no),
        };
        // Rendered through `Debug` on purpose and for now: the report has no
        // wording of its own yet, and inventing one here would be inventing
        // it twice.
        let report = status::status(client.output().as_path());
        Ok(say("status", vec![format!("{report:#?}")]))
    }

    /// Prove the executor is alive by getting a reply out of it.
    #[tool]
    async fn dcs_ping(
        &self,
        Parameters(args): Parameters<Ping>,
    ) -> Result<CallToolResult, ErrorData> {
        let client = match client_for(self, args.host.as_deref()) {
            Ok(client) => client,
            Err(no) => return Ok(no),
        };
        let stamp = client.handshake().stamp.clone();
        let spec = Spec::new(&[("op", "ping"), ("for", &stamp)], b"");
        Ok(one(&client, spec, waiting(args.wait_seconds)))
    }

    /// What the game is doing, every fact it rests on, and the basis of each
    /// value.
    #[tool]
    async fn dcs_game_state(
        &self,
        Parameters(args): Parameters<GameState>,
    ) -> Result<CallToolResult, ErrorData> {
        let client = match client_for(self, args.host.as_deref()) {
            Ok(client) => client,
            Err(no) => return Ok(no),
        };
        let upto = waiting(args.wait_seconds);
        Ok(
            match game::game_state(client.output().as_path(), Tiers::default(), upto) {
                Ok(state) => say("game-state", vec![state.to_string()]),
                Err(why) => refuse("refused", vec![why.to_string()]),
            },
        )
    }

    /// Run a chunk of Lua inside the running game and report what it came to.
    #[tool]
    async fn dcs_eval(
        &self,
        Parameters(args): Parameters<Eval>,
    ) -> Result<CallToolResult, ErrorData> {
        let client = match client_for(self, args.host.as_deref()) {
            Ok(client) => client,
            Err(no) => return Ok(no),
        };
        let stamp = client.handshake().stamp.clone();
        let budget = args.max_instructions.map(|max| max.to_string());
        let mut headers = vec![
            ("op", "eval"),
            ("for", stamp.as_str()),
            ("state", args.state.as_str()),
        ];
        if let Some(name) = &args.chunkname {
            headers.push(("chunkname", name.as_str()));
        }
        if let Some(budget) = &budget {
            headers.push(("max_instructions", budget.as_str()));
        }
        let spec = Spec::new(&headers, args.code.as_bytes());
        Ok(one(&client, spec, waiting(args.wait_seconds)))
    }

    /// The same, over a file this server reads. The path is judged before any
    /// byte of it is opened, and the answer carries where the bytes came from
    /// and what they hashed to.
    #[tool]
    async fn dcs_eval_file(
        &self,
        Parameters(args): Parameters<EvalFile>,
    ) -> Result<CallToolResult, ErrorData> {
        let client = match client_for(self, args.host.as_deref()) {
            Ok(client) => client,
            Err(no) => return Ok(no),
        };
        let refused = |why: String| Ok(refuse("refused", vec![why]));
        let real = match paths::resolve(Path::new(&args.path)) {
            Ok(real) => real,
            Err(why) => return refused(why.to_string()),
        };
        // No root is allowed, because nothing configures one yet, and an
        // empty allowed list admits nothing rather than everything. Every
        // path is refused here until a root can be named.
        let roots = match Roots::new(&[], &writedirs(self), None) {
            Ok(roots) => roots,
            Err(why) => return refused(why.to_string()),
        };
        let chunkname = match source::chunkname(&real) {
            Ok(name) => name,
            Err(why) => return refused(why.to_string()),
        };
        let stamp = client.handshake().stamp.clone();
        let budget = args.max_instructions.map(|max| max.to_string());
        let mut headers = vec![
            ("op", "eval"),
            ("for", stamp.as_str()),
            ("state", args.state.as_str()),
            ("chunkname", chunkname.as_str()),
        ];
        if let Some(budget) = &budget {
            headers.push(("max_instructions", budget.as_str()));
        }
        // The same header slice measures the file and frames the request, so
        // the ceiling the file was admitted under is the one the request is
        // really written against.
        let admitted = match file::check(&roots, client.handshake(), &headers, &real) {
            Ok(admitted) => admitted,
            Err(why) => return refused(why.to_string()),
        };
        let source = match source::read(&admitted) {
            Ok(source) => source,
            Err(why) => return refused(why.to_string()),
        };
        let spec = Spec::new(&headers, source.body());
        let mut answer = one(&client, spec, waiting(args.wait_seconds));
        // Which bytes ran, said beside the answer rather than instead of it.
        answer.content.push(ContentBlock::text(format!(
            "source: {}\nsha256: {}",
            source.path(),
            source.sha256_hex()
        )));
        Ok(answer)
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
            Ok(Collected::Reply(envelope)) => say("reply", reply_lines(&envelope)),
            Ok(Collected::Foreign { saw, wanted }) => refuse(
                "stale-session",
                vec![format!(
                    "the reply under {} is stamped {saw}, and the session addressed is {wanted}",
                    args.id
                )],
            ),
            Ok(Collected::Nothing) => say(
                "pending",
                vec![
                    format!("id: {}", args.id),
                    "nothing has landed under that id yet".to_owned(),
                ],
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
