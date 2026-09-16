use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let command = args.first().map(String::as_str).unwrap_or("help");

    match command {
        "validate" => validate(root_arg(&args, 1)),
        "inspect" => inspect(root_arg(&args, 1)),
        "github" => github(&args),
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

fn github(args: &[String]) -> ExitCode {
    let subcommand = args.get(1).map(String::as_str).unwrap_or("help");
    match subcommand {
        "plan" => github_plan(root_arg(args, 2), args.get(3).map(String::as_str).unwrap_or("github")),
        "help" | "--help" | "-h" => {
            print_github_help();
            ExitCode::SUCCESS
        }
        other => {
            eprintln!("unknown github command: {other}\n");
            print_github_help();
            ExitCode::from(2)
        }
    }
}

fn github_plan(root: PathBuf, remote_name: &str) -> ExitCode {
    match allodium_core::github::plan_issues(&root, remote_name)
        .and_then(|plan| allodium_core::github::plan_to_toml(&plan))
    {
        Ok(plan) => {
            print!("{plan}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn root_arg(args: &[String], index: usize) -> PathBuf {
    PathBuf::from(args.get(index).map(String::as_str).unwrap_or("."))
}

fn print_help() {
    println!("allodium - filesystem-authoritative project state");
    println!();
    println!("USAGE:");
    println!("  allodium validate [ROOT]");
    println!("  allodium inspect  [ROOT]");
    println!("  allodium github plan [ROOT] [REMOTE]");
}

fn print_github_help() {
    println!("allodium github - GitHub projection tools");
    println!();
    println!("USAGE:");
    println!("  allodium github plan [ROOT] [REMOTE]");
}
