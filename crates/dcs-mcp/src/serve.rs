//! The serve role: what the server is pointed at, and how it finds the
//! executor each time it is asked.
//!
//! The one idea here is that finding the executor is not a start-up step. The
//! directory the executor writes into does not exist until DCS has loaded the
//! executor at least once, which is normally *after* a user has installed it
//! and pointed a client at this binary. A server that resolved everything up
//! front would have to be restarted before it could answer anything, and the
//! restart would look to the user like the install having failed. So the
//! server holds the options and nothing else, and [`Serve::client`] resolves
//! afresh on every call.

use std::fmt;
use std::path::{Path, PathBuf};

use dcs_eval::paths::{self, Real};
use dcs_eval::readers::Handshake;
use dcs_eval::wait::Session;
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::model::{Implementation, ServerCapabilities, ServerConfig};
use rmcp::transport::stdio;
use rmcp::{ServerHandler, ServiceExt};

/// Which of the executor's two hosts to talk to. Each writes into its own
/// directory, so the word is part of where the client looks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Host {
    Hook,
    Export,
}

impl Host {
    /// The word the executor spells the host with, which is also the last
    /// segment of its output directory.
    pub fn word(self) -> &'static str {
        match self {
            Host::Hook => "hook",
            Host::Export => "export",
        }
    }
}

/// What the server was pointed at.
///
/// `Saved Games` is given rather than discovered. The installer finds it and
/// prints a `serve` line naming what it found; a client configuration names
/// one install rather than rediscovering it on every call, so here the flags
/// are required and a missing one is a usage line rather than a guess.
#[derive(Debug, Clone)]
pub struct Options {
    pub saved_games: PathBuf,
    pub variant: String,
    pub host: Host,
    /// Where this build keeps what is its own: the run record every
    /// evaluation appends to, and a reply the command line was told to
    /// capture. Nothing where the known folder is to be used, which is the
    /// ordinary case; a path here is an override, and it is judged against
    /// the DCS write directory exactly as the known folder is.
    pub data_dir: Option<PathBuf>,
}

impl Options {
    /// Where the executor writes, spelt the way the executor's own Lua joins
    /// it. Nothing here asks the filesystem, so this answers before the
    /// directory exists.
    pub fn output(&self) -> PathBuf {
        output_in(&self.saved_games.join(&self.variant), self.host)
    }

    /// `--saved-games <dir> --variant <name> [--host hook|export]
    /// [--data-dir <dir>]`, and nothing else.
    ///
    /// Hand-rolled and deliberately small. The install's flags are the
    /// installer's, not these. An unrecognised flag is refused by name rather
    /// than accepted and ignored, so a user who passes one this verb does not
    /// take is told so.
    ///
    /// A flag given twice is refused for the same reason. Last-wins is the
    /// usual answer, but nothing here takes a list, so a repeat is a client
    /// configuration with two opinions about which install to talk to, and the
    /// half that is ignored would be ignored silently.
    pub fn parse<I: IntoIterator<Item = String>>(args: I) -> Result<Self, String> {
        let mut saved_games = None;
        let mut variant = None;
        let mut host = None;
        let mut data_dir = None;
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            let mut value = |name: &str| {
                args.next()
                    .ok_or_else(|| format!("{name} wants a value after it"))
            };
            match arg.as_str() {
                "--saved-games" => once(
                    &mut saved_games,
                    "--saved-games",
                    PathBuf::from(value("--saved-games")?),
                )?,
                "--variant" => once(&mut variant, "--variant", value("--variant")?)?,
                "--host" => {
                    let given = value("--host")?;
                    let word = host_of(&given)
                        .ok_or_else(|| format!("--host is hook or export, not {given}"))?;
                    once(&mut host, "--host", word)?
                }
                "--data-dir" => once(
                    &mut data_dir,
                    "--data-dir",
                    PathBuf::from(value("--data-dir")?),
                )?,
                other => return Err(format!("serve does not take {other}")),
            }
        }
        Ok(Self {
            saved_games: saved_games.ok_or("serve wants --saved-games <dir>")?,
            variant: variant.ok_or("serve wants --variant <name>")?,
            host: host.unwrap_or(Host::Hook),
            data_dir,
        })
    }

    /// The same options aimed at another host.
    ///
    /// A call may name a host of its own, and the executor's two hosts write
    /// into two directories, so a call that names one is asking to be
    /// resolved somewhere other than where the flag points. Everything else
    /// about the install is the same, which is why this replaces one field
    /// rather than taking a second set of flags.
    pub fn at_host(&self, host: Host) -> Options {
        Options {
            host,
            ..self.clone()
        }
    }
}

/// Where the executor of `host` writes under a variant directory already in
/// hand.
///
/// The join lives here and not in each caller so that a caller holding a
/// resolved variant reports paths under it in the one spelling. A second
/// join composed from the flags would reach the same directory by a
/// different name, and a report naming a resolved path beside an
/// unresolved one reads as two places.
pub fn output_in(variant: &Path, host: Host) -> PathBuf {
    variant.join("Logs").join("DcsEval").join(host.word())
}

/// The host a word names, or nothing where it names neither.
///
/// The flag and a call's `host` argument accept the same two words. Spelling
/// them in two places is how the two would come to disagree, so the refusal
/// stays with each caller and the words stay here.
pub fn host_of(word: &str) -> Option<Host> {
    match word {
        "hook" => Some(Host::Hook),
        "export" => Some(Host::Export),
        _ => None,
    }
}

/// Fill a slot that has not been filled, or name the flag that filled it.
fn once<T>(slot: &mut Option<T>, flag: &str, value: T) -> Result<(), String> {
    if slot.is_some() {
        return Err(format!("{flag} is given twice"));
    }
    *slot = Some(value);
    Ok(())
}

/// No executor session could be addressed, and where the looking was done.
///
/// The reason is rendered to a `String` at the point it is raised rather than
/// carried as the error that caused it. A caller holding one of these has to
/// be able to hand out copies of it — every tool call that fails to find the
/// executor says the same thing — and `std::io::Error` is not `Clone`.
#[derive(Debug, Clone)]
pub struct NoSession {
    pub looked_in: PathBuf,
    pub why: String,
}

impl NoSession {
    fn at(looked_in: &Path, why: impl fmt::Display) -> Self {
        Self {
            looked_in: looked_in.to_owned(),
            why: why.to_string(),
        }
    }
}

impl fmt::Display for NoSession {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.looked_in.display(), self.why)
    }
}

impl std::error::Error for NoSession {}

/// One executor session, found. Everything a tool call needs to speak to it.
#[derive(Debug, Clone)]
pub struct Client {
    output: Real,
    handshake: Handshake,
    session: Session,
}

impl Client {
    /// Read the handshake the options point at and address the session it
    /// describes. Every refusal names the directory that was looked in, since
    /// the commonest cause is a `--saved-games` or `--variant` that is not the
    /// install the game is actually running.
    pub fn resolve(opts: &Options) -> Result<Client, NoSession> {
        let wanted = opts.output();
        let output = paths::resolve(&wanted).map_err(|why| NoSession::at(&wanted, why))?;
        let handshake = Handshake::read(&output.as_path().join("executor.txt"))
            .map_err(|why| NoSession::at(&wanted, why))?;
        // Every call that publishes comes through here, so a transport this
        // client must not write into is refused before anything is written.
        let writedir = paths::resolve(&opts.saved_games.join(&opts.variant)).ok();
        if let Some(why) = handshake.unwritable(writedir.as_ref()) {
            return Err(NoSession::at(&wanted, why));
        }
        let session = Session::addressed(&handshake);
        Ok(Client {
            output,
            handshake,
            session,
        })
    }

    pub fn output(&self) -> &Real {
        &self.output
    }

    pub fn handshake(&self) -> &Handshake {
        &self.handshake
    }

    pub fn session(&self) -> &Session {
        &self.session
    }
}

/// The server: the options, the tools it answers over them, and nothing
/// resolved.
#[derive(Debug, Clone)]
pub struct Serve {
    opts: Options,
    /// The one router. What is registered in it is what is listed, because
    /// the listing is taken from it and there is no second list to fall out
    /// of step with.
    tools: ToolRouter<Serve>,
}

impl Serve {
    /// Hold the options. Deliberately does no looking: see the module note.
    pub fn new(opts: Options) -> Self {
        Self {
            opts,
            tools: Self::tool_router(),
        }
    }

    pub fn options(&self) -> &Options {
        &self.opts
    }

    /// The executor, as it is right now.
    ///
    /// Every tool call goes through this one function, and it resolves afresh
    /// each time. That is the whole point: the output directory appears the
    /// first time DCS loads the executor, which can be long after this process
    /// started, and a session that ends is replaced by one with a different
    /// stamp. Anything cached here would be wrong in both cases.
    ///
    /// The reads are blocking, and a handler that awaits will block its thread
    /// over them. They are two small local files, so this is a choice rather
    /// than an oversight.
    pub fn client(&self) -> Result<Client, NoSession> {
        Client::resolve(&self.opts)
    }

    /// The executor for a host other than the one the flag names, which is
    /// what a call's own `host` argument asks for. Resolved afresh, for the
    /// reason the module note gives.
    pub fn client_at(&self, host: Host) -> Result<Client, NoSession> {
        Client::resolve(&self.opts.at_host(host))
    }
}

#[rmcp::tool_handler(router = self.tools)]
impl ServerHandler for Serve {
    fn get_info(&self) -> ServerConfig {
        let mut info = ServerConfig::new(ServerCapabilities::builder().enable_tools().build());
        info.server_info = Implementation::new("dcs-mcp", env!("CARGO_PKG_VERSION"));
        // The capability is claimed because the tools are registered, and
        // what is listed comes off the same router they are registered in.
        //
        // The wording below is a placeholder and is known to be one: how a
        // reply reads — a refusal that reads as a refusal, a `pending` that
        // names its id and phase — is settled in one place, and not here.
        // It is a clause per tool and no more, so that it does not become a
        // second copy of the README's prose for that work to keep in step.
        info.instructions = Some(
            "Evaluate Lua inside a running DCS World. `dcs_status` reports what \
             is readable without asking the executor anything; `dcs_ping` proves \
             it is alive; `dcs_game_state` says what the game is doing; \
             `dcs_eval` and `dcs_eval_file` run a chunk; `dcs_collect` picks up \
             a reply left pending."
                .to_owned(),
        );
        info
    }
}

/// Speak MCP over stdin and stdout until the client hangs up.
///
/// One current-thread runtime, because the work behind every call is a
/// handful of small local file reads. Tokio's stdin and stdout are served by
/// its blocking pool rather than by the IO driver, so the IO driver stays off;
/// a compiler or a panic that disagrees is naming a driver to enable, not a
/// reason to reach for `enable_all`.
///
/// The timer is on for rmcp rather than for anything written here: it puts a
/// timeout around the end of a session, and on a runtime without timers that
/// panics *after* the last frame has been written — a client sees a clean
/// conversation and a crashed server.
pub fn run(opts: Options) -> Result<(), Box<dyn std::error::Error>> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()?;
    runtime.block_on(async move {
        // The one start-up line, and it says where the executor will be looked
        // for rather than what was found there: nothing is resolved yet, and
        // a wrong `--saved-games` is the commonest thing to get wrong.
        tracing::info!(
            "serving MCP over stdio; the executor is looked for afresh on every call, in {}",
            opts.output().display()
        );
        let service = Serve::new(opts).serve(stdio()).await?;
        service.waiting().await?;
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::Sandbox;
    use dcs_eval::standin::Standin;

    /// The options a test is driven over, with the data directory inside the
    /// box: every evaluation writes a record, and a fixture left pointing at
    /// the known folder would write it into the machine's own.
    fn opts(box_: &Sandbox, host: Host) -> Options {
        Options {
            saved_games: box_.path.clone(),
            variant: "DCS.openbeta".to_owned(),
            host,
            data_dir: Some(box_.join("data")),
        }
    }

    /// The executor's directory is published by the executor, so it appears
    /// while the server is already running. The second call must see what the
    /// first one could not.
    #[test]
    fn a_root_that_appears_between_two_calls_is_resolved_by_the_second() {
        let box_ = Sandbox::new();
        let opts = opts(&box_, Host::Hook);
        let serve = Serve::new(opts.clone());

        let why = serve
            .client()
            .expect_err("nothing has loaded the executor yet")
            .to_string();
        assert!(
            why.contains("DcsEval"),
            "the refusal names the directory it looked in: {why}"
        );

        // DCS loads the executor: the stand-in makes the tree itself and
        // publishes a handshake into it.
        let ex = Standin::open(&opts.output(), "hook").expect("the stand-in opens");
        ex.handshake().expect("the handshake is published");

        let second = serve
            .client()
            .expect("the second call resolves the root that appeared between them");
        assert_eq!(second.session().stamp(), ex.stamp);
        assert_eq!(
            second.session().res(),
            paths::resolve(ex.res())
                .expect("the reply directory resolves")
                .as_path(),
            "the session is addressed at the directory the handshake named"
        );
    }

    /// A session that ends is replaced by one with a different stamp, in the
    /// same output directory. Nothing announces that to the server, so the only
    /// way it can be right is by reading the handshake again — which is what a
    /// client kept after the first success would not do.
    #[test]
    fn a_session_replaced_by_one_with_another_stamp_is_picked_up() {
        let box_ = Sandbox::new();
        let opts = opts(&box_, Host::Hook);
        let serve = Serve::new(opts.clone());

        let mut ex = Standin::open(&opts.output(), "hook").expect("the stand-in opens");
        ex.handshake().expect("the handshake is published");
        let first = serve.client().expect("the first call finds the session");
        assert_eq!(first.session().stamp(), ex.stamp);

        // DCS reloads: the executor mints a new stamp and republishes the
        // handshake over the old one. The stand-in mints its directories at
        // open and keeps them, which is beside the point here — what the server
        // addresses a session on is the stamp the handshake names.
        let restarted = format!("{}-again", ex.stamp);
        ex.stamp = restarted.clone();
        ex.handshake()
            .expect("the reloaded executor republishes the handshake");

        let second = serve.client().expect("the second call finds the session");
        assert_eq!(
            second.session().stamp(),
            restarted,
            "the second call read the handshake again rather than answering out of the first"
        );
    }

    /// A handshake naming a transport in the variant's `Config` is refused
    /// before a call publishes anything there, and the refusal says which
    /// path and why.
    #[test]
    fn a_transport_outside_logs_is_refused_before_anything_is_written() {
        let box_ = Sandbox::new();
        let opts = opts(&box_, Host::Hook);
        let serve = Serve::new(opts.clone());
        let ex = Standin::open(&opts.output(), "hook").expect("the stand-in opens");
        ex.handshake().expect("the handshake is published");
        let config = box_.join("DCS.openbeta").join("Config").join("rpc");
        for dir in ["req", "res"] {
            std::fs::create_dir_all(config.join(dir)).expect("somewhere a write would land");
        }
        let handshake = opts.output().join("executor.txt");
        crate::testing::reheader(&handshake, "transport", &config.display().to_string());
        for name in ["req", "res", "arm"] {
            let value = config.join(name).display().to_string();
            crate::testing::reheader(&handshake, name, &value);
        }

        let why = serve
            .client()
            .expect_err("the transport is refused")
            .to_string();
        assert!(why.contains("transport: "), "{why}");
        assert!(why.contains("not under its Logs"), "{why}");

        let answered = crate::tools::ping(&serve, None, std::time::Duration::from_millis(50));
        assert_eq!(answered.answer.is_error, Some(true), "a ping is refused");
        for dir in ["req", "res"] {
            let left = std::fs::read_dir(config.join(dir)).expect("listed").count();
            assert_eq!(left, 0, "nothing was written into {dir}");
        }
        assert!(!config.join("arm").exists(), "and nothing armed");
    }

    #[test]
    fn a_flag_given_twice_is_refused_by_name() {
        let args = |v: &[&str]| v.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        let good = args(&[
            "--saved-games",
            "C:\\sg",
            "--variant",
            "DCS.openbeta",
            "--host",
            "export",
            "--data-dir",
            "C:\\data",
        ]);
        let parsed = Options::parse(good.clone()).expect("the four flags parse");
        assert_eq!(parsed.host, Host::Export);
        assert_eq!(parsed.data_dir.as_deref(), Some(Path::new("C:\\data")));

        for (flag, value) in [
            ("--saved-games", "C:\\other"),
            ("--variant", "DCS"),
            ("--host", "hook"),
            ("--data-dir", "C:\\elsewhere"),
        ] {
            let mut twice = good.clone();
            twice.push(flag.to_owned());
            twice.push(value.to_owned());
            let why = Options::parse(twice).expect_err("a repeated flag is refused");
            assert!(
                why.contains(flag) && why.contains("twice"),
                "the refusal names the flag that was repeated: {why}"
            );
        }
    }

    #[test]
    fn the_output_directory_is_where_the_executor_writes_it() {
        let box_ = Sandbox::new();
        for host in [Host::Hook, Host::Export] {
            assert_eq!(
                opts(&box_, host).output(),
                box_.path
                    .join("DCS.openbeta")
                    .join("Logs")
                    .join("DcsEval")
                    .join(host.word())
            );
        }
    }

    /// Two servers over the same options answer out of the filesystem, not out
    /// of anything either of them kept. A cache shared between them — or one
    /// per instance, reached before the tree existed — would make the second
    /// one wrong.
    #[test]
    fn a_second_serve_over_the_same_options_resolves_independently() {
        let box_ = Sandbox::new();
        let opts = opts(&box_, Host::Hook);
        let early = Serve::new(opts.clone());
        early
            .client()
            .expect_err("the early server looks before there is anything to find");

        let ex = Standin::open(&opts.output(), "hook").expect("the stand-in opens");
        ex.handshake().expect("the handshake is published");

        let late = Serve::new(opts);
        let found = late
            .client()
            .expect("a server asked after the executor loaded finds it");
        assert_eq!(found.session().stamp(), ex.stamp);
    }
}
