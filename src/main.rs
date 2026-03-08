//! `LeekScript` CLI: format, validate, and other source manipulations.

mod cli;

use clap::Parser;

fn main() {
    // Install graphical report handler so parse errors are pretty-printed with source snippets.
    let _ = miette::set_hook(Box::new(
        |_| Box::new(miette::GraphicalReportHandler::new()),
    ));

    let cli = cli::Cli::parse();
    let code = match cli.command {
        cli::Commands::Format(ref args) => cli::run_format(args),
        cli::Commands::Validate(ref args) => cli::run_validate(args),
    };
    std::process::exit(code);
}
