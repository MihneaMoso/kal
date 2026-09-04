//! Probe: do two same-chain [`IrohTransport`]s discover each other through the
//! real DHT + iroh.link + relay stack?
//!
//! This reproduces the phone↔desktop sync scenario on a single machine: two
//! transports derived from the same recovery phrase, with distinct node
//! secrets, waiting for the gossip topic to connect. If these two cannot find
//! each other, the phone will never sync either, and the trace shows exactly
//! which hop fails (record publish → record fetch → address resolution →
//! QUIC/relay connection).
//!
//! Run with:
//! ```sh
//! cargo run -p kal-sync --example sync_probe
//! ```
//!
//! Exit code 0 = both joined and A's broadcast reached B. Exit code 1 =
//! discovery or delivery failed (details printed + traced).

use std::process::ExitCode;
use std::thread;
use std::time::{Duration, Instant};

use kal_sync::live::IrohTransport;
use kal_sync::{ChainIdentity, Transport as _};

const PHRASE: &str =
    "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

fn main() -> ExitCode {
    // Trace everything at DEBUG level so the full bootstrap path (record
    // publish/fetch, per-peer join attempts, address resolution, relay/direct
    // connect) is visible on stdout.
    use tracing_subscriber::filter::LevelFilter;
    let _ = tracing_subscriber::fmt()
        .with_max_level(LevelFilter::DEBUG)
        .with_ansi(false)
        .with_target(true)
        .with_thread_names(true)
        .try_init();

    let identity = match ChainIdentity::from_phrase(PHRASE) {
        Ok(id) => id,
        Err(e) => {
            eprintln!("bad phrase: {e}");
            return ExitCode::from(2);
        }
    };
    println!(
        "chain fingerprint: {}",
        identity.fingerprint().fingerprint_hex
    );

    let a_id = ulid::Ulid::new();
    let b_id = ulid::Ulid::new();

    // connect() drives a tokio runtime with block_on internally, so it must
    // not be called from inside a runtime — dedicated threads, like the app.
    let ident_a = identity.clone();
    let ident_b = identity;
    let a_join = thread::spawn(move || {
        IrohTransport::connect(&ident_a, a_id, IrohTransport::new_node_secret())
    });
    let b_join = thread::spawn(move || {
        IrohTransport::connect(&ident_b, b_id, IrohTransport::new_node_secret())
    });

    let a = match a_join.join() {
        Ok(Ok(a)) => a,
        Ok(Err(e)) => {
            eprintln!("transport A failed to connect: {e}");
            return ExitCode::from(2);
        }
        Err(_) => return ExitCode::from(2),
    };
    let b = match b_join.join() {
        Ok(Ok(b)) => b,
        Ok(Err(e)) => {
            eprintln!("transport B failed to connect: {e}");
            return ExitCode::from(2);
        }
        Err(_) => return ExitCode::from(2),
    };
    println!("A endpoint: {}", a.endpoint_id());
    println!("B endpoint: {}", b.endpoint_id());
    println!("waiting for gossip peers to discover each other (up to 3 min)…");

    let start = Instant::now();
    let deadline = start + Duration::from_secs(180);
    let mut aj = false;
    let mut bj = false;
    loop {
        aj = a.is_joined();
        bj = b.is_joined();
        println!(
            "[t={:>4}s] A joined={aj:<5} B joined={bj}",
            start.elapsed().as_secs()
        );
        if aj && bj {
            break;
        }
        if Instant::now() >= deadline {
            eprintln!(
                "TIMEOUT: peers never joined after 180s (final A joined={aj}, B joined={bj})"
            );
            return ExitCode::from(1);
        }
        thread::sleep(Duration::from_secs(5));
    }
    println!("both joined! exchanging a state frame A → B…");

    // Send a REALISTIC payload, not a toy hello: a production sealed state is
    // tens of KiB (37 KiB sealed → ~132 KiB JSON frame for 32 items), and
    // iroh-gossip's 4 KiB default limit used to silently drop such frames
    // while the tiny hello below sailed through — so the probe passed while
    // real sync never converged. A 200 KiB payload guards that bug class.
    let mut payload = vec![0u8; 200 * 1024];
    for (i, b) in payload.iter_mut().enumerate() {
        *b = (i % 251) as u8;
    }
    if let Err(e) = a.send("", &payload) {
        eprintln!("A broadcast failed: {e}");
        return ExitCode::from(1);
    }
    let t0 = Instant::now();
    loop {
        if let Some((from, bytes)) = b.recv() {
            println!("B received {} bytes from {}", bytes.len(), from);
            if bytes == payload {
                break;
            }
            eprintln!(
                "MISMATCH: B received {} bytes, expected {}",
                bytes.len(),
                payload.len()
            );
            return ExitCode::from(1);
        }
        if t0.elapsed() > Duration::from_secs(30) {
            eprintln!("TIMEOUT: B never received A's broadcast");
            return ExitCode::from(1);
        }
        thread::sleep(Duration::from_millis(250));
    }
    println!("PROBE OK");
    ExitCode::SUCCESS
}
