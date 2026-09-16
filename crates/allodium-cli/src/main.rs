use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = env::args().skip(1);
    let command = args.next().unwrap_or_else(|| "help".into());
    let root = PathBuf::from(args.next().unwrap_or_else(|| ".".into()));

    match command.as_str() {
        "validate" => validate(root),
        "inspect" => inspect(root),
        "help" | "--help" | "-h" => {
            print_help();
            ExitCode::SUCCESS
        }
        other => {
            eprintln!("unknown command: {other}\n");
            print_help();
            ExitCode::from(2)
        }
    }
}

fn validate(root: PathBuf) -> ExitCode {
    let report = allodium_core::validate(&root);
    for warning in &report.warnings {
        eprintln!("warning: {warning}");
    }
    for error in &report.errors {
        eprintln!("error: {error}");
    }
    if report.is_ok() {
        println!("Allodium project state is valid: {}", root.display());
        ExitCode::SUCCESS
    } else {
        eprintln!("validation failed with {} error(s)", report.errors.len());
        ExitCode::FAILURE
    }
}

fn inspect(root: PathBuf) -> ExitCode {
    match allodium_core::load_manifest(&root) {
        Ok(manifest) => {
            println!("{} ({})", manifest.name, manifest.id);
            if !manifest.description.is_empty() {
                println!("{}", manifest.description);
            }
            println!("schema: {}", manifest.schema);
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn print_help() {
    println!("allodium - filesystem-authoritative project state");
    println!();
    println!("USAGE:");
    println!("  allodium validate [ROOT]");
    println!("  allodium inspect  [ROOT]");
}
