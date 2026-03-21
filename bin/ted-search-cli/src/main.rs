use std::process::ExitCode;

use clap::Parser;
use colored::Colorize;
use ted_search::{Algorithm, LabelDict};

use std::time::{Duration, Instant};

use console::{style, Emoji};
use indicatif::{HumanDuration, ProgressBar};

mod cli;
use cli::Cli;

static ROCKET: Emoji<'_, '_> = Emoji("🚀", "==>");
static TREE: Emoji<'_, '_> = Emoji("🌲", "DT");
static QUERY: Emoji<'_, '_> = Emoji("🔎", "QT");

fn main() -> ExitCode {
    let cli = Cli::parse();

    let lower_bound_method: Algorithm = cli.algorithm();

    let mut label_dict = LabelDict::new();

    println!("{} Parsing dataset...", TREE);
    let pb = ProgressBar::new_spinner();
    pb.enable_steady_tick(Duration::from_millis(100));
    let Ok(data_trees) = ted_search::parse_dataset(&cli.dataset, &mut label_dict) else {
        eprintln!(
            "{}",
            "Failed to parse dataset. Please check the dataset file path.".red()
        );
        return ExitCode::FAILURE;
    };
    pb.finish();
    println!(
        "{} {} {}",
        "Parsed".green(),
        data_trees.len().to_string().yellow(),
        "trees from dataset.".green()
    );

    println!("{} Parsing queries...", QUERY);
    let pb = ProgressBar::new_spinner();
    pb.enable_steady_tick(Duration::from_millis(100));
    let Ok(query_trees) = ted_search::parse_queries(&cli.queries, &mut label_dict, cli.delimiter)
    else {
        eprintln!(
            "{}",
            "Failed to parse queries. Please check the queries file path.".red()
        );
        return ExitCode::FAILURE;
    };

    println!(
        "{} {} {}",
        "Parsed".green(),
        query_trees.len().to_string().yellow(),
        "query trees.".green()
    );

    println!("{} Running search...", ROCKET);
    let pb = ProgressBar::new_spinner();
    pb.enable_steady_tick(Duration::from_millis(100));
    let mut each_try_time: Vec<Duration> = Vec::with_capacity(cli.runs);
    let total_start = Instant::now();
    for i in 1..=cli.runs {
        println!(
            "{} {} Times searched",
            style(format!("{i}/{runs}", runs = cli.runs)).bold().dim(),
            ROCKET
        );
        let start = Instant::now();
        lower_bound_method.search(&data_trees, &query_trees);
        each_try_time.push(start.elapsed());
    }
    let total_duration = total_start.elapsed();
    pb.finish_with_message("Search completed.");

    println!(
        "{} Search completed in {} (ran {} times, best time {})",
        ROCKET,
        style(HumanDuration(total_duration)).green().bold(),
        cli.runs,
        style(HumanDuration(
            *each_try_time
                .iter()
                .min()
                .expect("Unable to get minimum duration")
        ))
        .green()
        .bold()
    );

    ExitCode::SUCCESS
}
