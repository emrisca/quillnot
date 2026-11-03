//! A headless check that the app can actually talk to OpenRouter.
//!
//! Run `target/debug/quillnot --self-test`. It exists because the keychain entry is
//! scoped to the binary that created it: a `cargo test` process is a different
//! identity, and macOS will prompt for access — or refuse — so the honest place to
//! test the live path is the binary the person signed in with.
//!
//! Three things are checked, in the order a failure would matter: is a key present,
//! does the catalogue parse against our structs, does a real rewrite come back and
//! does the fallback chain reach the router when a model does not exist.

use std::time::Instant;

use crate::{catalog, openrouter, prompt, providers, store};

const SAMPLE: &str =
    "The library is very important for the team, and it shows a lot of promise for the next release.";

pub fn run() -> i32 {
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(err) => {
            eprintln!("could not start the runtime: {err}");
            return 1;
        }
    };

    runtime.block_on(async {
        if std::env::args().any(|arg| arg == "--rank") {
            return rank_table().await;
        }

        println!("QuillNot self-test");
        // `--long` adds a thousand-word run, which is the only way to exercise the
        // block splitter against a real model instead of a fixture.
        let long = std::env::args().any(|arg| arg == "--long");

        let status = store::key_status();
        match (&status.connected, &status.hint) {
            (true, Some(hint)) => println!("  account:    connected ({hint})"),
            _ => {
                println!("  account:    NOT connected — sign in from Settings first");
                return 1;
            }
        }

        match catalog::free_models_headless().await {
            Ok(models) => {
                println!("  catalogue:  {} zero-priced text models", models.len());
                for model in models.iter().take(4) {
                    println!("              · {}", model.id);
                }
            }
            Err(err) => {
                println!("  catalogue:  FAILED — {err}");
                return 1;
            }
        }

        let started = Instant::now();
        let request = openrouter::ParaphraseRequest {
            text: SAMPLE.to_string(),
            mode: Some("formal".to_string()),
            strength: Some(3),
            model: None,
        };
        match openrouter::paraphrase_with(None, request).await {
            Ok(result) => {
                println!(
                    "  rewrite:    {} words from {} block(s) in {:.1}s",
                    result.text.split_whitespace().count(),
                    result.blocks,
                    started.elapsed().as_secs_f32()
                );
                println!("              model: {}", result.model);
                println!("              out:   {}", result.text);
                if result.text.trim() == SAMPLE || result.text.trim().is_empty() {
                    println!("  rewrite:    FAILED — the model echoed the input or returned nothing");
                    return 1;
                }
            }
            Err(err) => {
                println!("  rewrite:    FAILED — {err}");
                return 1;
            }
        }

        // A model that cannot exist: the chain must land on the router instead of
        // surfacing an error.
        let (system, user) = prompt::paraphrase_prompt("Fallback path check.", "standard", 2);
        let key = match store::load_key() {
            Some(key) => key,
            None => {
                println!("  fallback:   FAILED — the key disappeared mid-test");
                return 1;
            }
        };
        // Bounded here as well as inside the client: a bad model id must not hold the
        // whole run open for the full three-minute block budget.
        let started = Instant::now();
        let base_url = "https://openrouter.ai/api/v1".to_string();
        let chain = [
            providers::Attempt {
                base_url: base_url.clone(),
                key: Some(key.clone()),
                model: "definitely-not-a-real-model:free".to_string(),
                openrouter: true,
            },
            providers::Attempt {
                base_url,
                key: Some(key),
                model: "openrouter/free".to_string(),
                openrouter: true,
            },
        ];
        let attempt = tokio::time::timeout(
            std::time::Duration::from_secs(60),
            openrouter::stream_block(None, 0, &chain, &system, &user),
        )
        .await;

        match attempt {
            Ok(Ok((model, body))) => println!(
                "  fallback:   answered by {model} ({} chars) in {:.1}s",
                body.chars().count(),
                started.elapsed().as_secs_f32()
            ),
            Ok(Err(err)) => {
                println!("  fallback:   FAILED — {err}");
                return 1;
            }
            Err(_) => {
                println!(
                    "  fallback:   TIMED OUT after {:.0}s — an unknown model id is not failing fast",
                    started.elapsed().as_secs_f32()
                );
                return 1;
            }
        }

        if long {
            let paragraph = "This release brings faster diagrams that match your theme, \
                reorderable filters in Bases, and new commands for adding content under a \
                heading. The diagrams are rendered by a new library that keeps up with the \
                latest syntax, they respond to the width of a note, and they can be zoomed \
                and panned when they are scaled down. Nothing in the note has to change to \
                benefit from any of it.";
            let document = vec![paragraph; 20].join("\n\n");
            let started = Instant::now();
            let request = openrouter::ParaphraseRequest {
                text: document.clone(),
                mode: Some("standard".to_string()),
                strength: Some(2),
                model: None,
            };

            match openrouter::paraphrase_with(None, request).await {
                Ok(result) => {
                    println!(
                        "  document:   {} blocks, {} words in, {} out, {:.1}s",
                        result.blocks,
                        document.split_whitespace().count(),
                        result.text.split_whitespace().count(),
                        started.elapsed().as_secs_f32()
                    );
                    if result.blocks < 2 {
                        println!("  document:   FAILED — a thousand words should have been chunked");
                        return 1;
                    }
                    if !result.text.contains("\n\n") {
                        println!("  document:   FAILED — the paragraph breaks were lost");
                        return 1;
                    }
                }
                Err(err) => {
                    println!("  document:   FAILED — {err}");
                    return 1;
                }
            }
        }

        0
    })
}

/// `--rank`: the same table the AI Models view renders, printed to a terminal so the
/// numbers can be checked without a window.
async fn rank_table() -> i32 {
    match catalog::ranked_models().await {
        Ok(rows) => {
            let scored = rows.iter().filter(|row| row.score.is_some()).count();
            println!("{} free text models, {scored} with live numbers", rows.len());
            println!(
                "{:<3} {:<44} {:>8} {:>9} {:>9} {:>6}",
                "#", "model", "uptime", "latency", "tput", "score"
            );
            for (index, row) in rows.iter().take(15).enumerate() {
                println!(
                    "{:<3} {:<44} {:>8} {:>9} {:>9} {:>6}",
                    index + 1,
                    row.id,
                    row.uptime
                        .map(|value| format!("{:.0}%", value * 100.0))
                        .unwrap_or_else(|| "—".to_string()),
                    row.latency_ms
                        .map(|value| format!("{value:.0}ms"))
                        .unwrap_or_else(|| "—".to_string()),
                    row.throughput
                        .map(|value| format!("{value:.0}/s"))
                        .unwrap_or_else(|| "—".to_string()),
                    row.score
                        .map(|value| format!("{value:.0}"))
                        .unwrap_or_else(|| "—".to_string()),
                );
            }
            0
        }
        Err(err) => {
            println!("ranking failed — {err}");
            1
        }
    }
}
