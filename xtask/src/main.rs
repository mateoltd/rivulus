//! xtask: local gate runner (`cargo xtask <cmd>`, mirrors CI order).
//! lint-guards stay CI-only (python in .github/workflows/ci.yml).

use std::process::{exit, Command};

fn run(args: &[&str], env: &[(&str, &str)]) -> i32 {
    let mut cmd = Command::new("cargo");
    cmd.args(args);
    for (k, v) in env {
        cmd.env(k, v);
    }
    match cmd.status() {
        Ok(s) if s.success() => 0,
        Ok(s) => s.code().unwrap_or(1),
        Err(e) => {
            eprintln!("xtask: cannot run cargo {args:?}: {e}");
            1
        }
    }
}

fn usage() -> ! {
    eprintln!("usage: cargo xtask <fmt|clippy|test|doc|all>");
    exit(2);
}

fn main() {
    const DOC_ENV: &[(&str, &str)] = &[("RUSTDOCFLAGS", "-D warnings")];
    type Step = (
        &'static [&'static str],
        &'static [(&'static str, &'static str)],
    );
    let cmd = std::env::args().nth(1).unwrap_or_else(|| usage());
    let code = match cmd.as_str() {
        "fmt" => run(&["fmt", "--check"], &[]),
        "clippy" => run(
            &[
                "clippy",
                "--workspace",
                "--all-features",
                "--",
                "-D",
                "warnings",
            ],
            &[],
        ),
        "test" => run(&["test", "--workspace", "--all-features"], &[]),
        "doc" => run(
            &["doc", "--workspace", "--no-deps", "--all-features"],
            DOC_ENV,
        ),
        "all" => {
            let mut code = run(&["fmt", "--check"], &[]);
            let steps: &[Step] = &[
                (
                    &[
                        "clippy",
                        "--workspace",
                        "--all-features",
                        "--",
                        "-D",
                        "warnings",
                    ],
                    &[],
                ),
                (&["test", "--workspace", "--all-features"], &[]),
                (
                    &["doc", "--workspace", "--no-deps", "--all-features"],
                    DOC_ENV,
                ),
            ];
            for (a, e) in steps {
                let c = run(a, e);
                if code == 0 {
                    code = c;
                }
            }
            code
        }
        _ => usage(),
    };
    exit(code);
}
