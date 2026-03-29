use std::{
    io::{BufWriter, Write},
    process::ExitCode,
};

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

    macro_rules! maybe_println {
        ($($arg:tt)*) => {
            if !cli.formatted {
                println!($($arg)*);
            }
        };
    }

    let lower_bound_method: Algorithm = cli.algorithm();

    let mut label_dict = LabelDict::new();

    maybe_println!("{} Parsing dataset...", TREE);
    let pb = (!cli.formatted).then(ProgressBar::new_spinner);
    if let Some(pb) = &pb {
        pb.enable_steady_tick(Duration::from_millis(100));
    }
    let Ok(data_trees) = ted_search::parse_dataset(&cli.dataset, &mut label_dict) else {
        eprintln!(
            "{}",
            "Failed to parse dataset. Please check the dataset file path.".red()
        );
        return ExitCode::FAILURE;
    };
    if let Some(pb) = pb {
        pb.finish();
    }
    maybe_println!(
        "{} {} {}",
        "Parsed".green(),
        data_trees.len().to_string().yellow(),
        "trees from dataset.".green()
    );

    maybe_println!("{} Parsing queries...", QUERY);
    let pb = (!cli.formatted).then(ProgressBar::new_spinner);
    if let Some(pb) = &pb {
        pb.enable_steady_tick(Duration::from_millis(100));
    }
    let Ok(query_trees) = ted_search::parse_queries(&cli.queries, &mut label_dict, cli.delimiter)
    else {
        eprintln!(
            "{}",
            "Failed to parse queries. Please check the queries file path.".red()
        );
        return ExitCode::FAILURE;
    };

    if let Some(pb) = pb {
        pb.finish();
    }
    maybe_println!(
        "{} {} {}",
        "Parsed".green(),
        query_trees.len().to_string().yellow(),
        "query trees.".green()
    );

    maybe_println!("{} Running search...", ROCKET);
    let pb = (!cli.formatted).then(ProgressBar::new_spinner);
    if let Some(pb) = &pb {
        pb.enable_steady_tick(Duration::from_millis(100));
    }
    let mut each_try_time: Vec<Duration> = Vec::with_capacity(cli.runs);
    let total_start = Instant::now();
    let mut search_results = vec![];
    let mut candidates_count = 0;
    for i in 1..=cli.runs {
        maybe_println!(
            "{} {} Times searched",
            style(format!("{i}/{runs}", runs = cli.runs)).bold().dim(),
            ROCKET
        );
        let start = Instant::now();
        search_results = lower_bound_method.search(&data_trees, &query_trees);
        each_try_time.push(start.elapsed());
        candidates_count = search_results.iter().map(|res| res.len()).sum();
    }
    let total_duration = total_start.elapsed();
    if let Some(pb) = pb {
        pb.finish_with_message("Search completed.");
    }

    maybe_println!(
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

    if cli.formatted {
        println!("{}", cli.method);
        println!("time:{}ms", each_try_time.iter().min().unwrap().as_millis());
        println!("candidates:{candidates_count}",);
    }

    if cli.output.is_some() {
        let output_path = cli.output.as_ref().unwrap();
        if !output_path.is_dir() {
            eprintln!(
                "{}",
                format!("Output path is not a directory: {}", output_path.display()).red()
            );
            return ExitCode::FAILURE;
        }
        let output_file = output_path.join(format!("{}_candidates.csv", cli.method));
        let out_file = std::fs::File::options()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&output_file)
            .unwrap_or_else(|_| {
                eprintln!(
                    "{}",
                    format!("Failed to create output file: {}", output_file.display()).red()
                );
                std::process::exit(1);
            });
        let mut writer = BufWriter::new(out_file);
        // write candidates to file
        for (query_idx, candidates) in search_results.iter().enumerate() {
            for &candidate in candidates {
                writer
                    .write_fmt(format_args!("{},{}\n", query_idx, candidate))
                    .unwrap_or_else(|_| {
                        eprintln!(
                            "{}",
                            format!("Failed to write to output file: {}", output_file.display())
                                .red()
                        );
                        std::process::exit(1);
                    });
            }
        }
    }

    ExitCode::SUCCESS
}
