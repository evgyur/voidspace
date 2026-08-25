use std::io::{self, BufReader};

use anyhow::Context;
use voidspace_elevated::{Request, Response, read_frame, turbo_mode_for, write_frame};

fn main() -> anyhow::Result<()> {
    if std::env::args().any(|argument| argument == "--probe") {
        println!("{{\"ready\":true,\"elevated\":true}}");
        return Ok(());
    }

    let request: Request = read_frame(BufReader::new(io::stdin().lock()))
        .context("reading one bounded request frame")?;
    let response = match request.kind {
        voidspace_elevated::RequestKind::Probe => Response::Ready { elevated: true },
        voidspace_elevated::RequestKind::TurboStart { root } => Response::TurboAccepted {
            mode: turbo_mode_for(&root),
        },
        _ => Response::Rejected {
            reason: "This packaged helper accepts probe and Turbo negotiation; destructive operations are executed only through the confirmed in-process manifest gate".into(),
        },
    };
    write_frame(io::stdout().lock(), &response).context("writing response")?;
    Ok(())
}
