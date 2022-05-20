extern crate multiqueue2 as multiqueue;

use clap::Parser;

use mcp::{copy_process, validate};

#[derive(Parser, Debug, Clone)]
#[clap(version,
    about = "Copy file to multiple targets",
    long_about = None)]
struct Args {
    #[clap(
        short,
        long,
        help = "Input file, for copy process",
        multiple_values = true,
        required = true
    )]
    input: Vec<String>,

    #[clap(
        short,
        long,
        help = "Output file, can be used multiple times",
        multiple_occurrences = true,
        required = true
    )]
    output: Vec<String>,

    #[clap(short, long, help = "Show progress")]
    pub progress: bool,
}

fn main() {
    let args = Args::parse();

    match validate(&args.input, &args.output) {
        Ok(_) => {
            if let Err(e) = copy_process(args.input, args.output, args.progress) {
                println!("{e}");
            }
        }
        Err(e) => println!("{e}"),
    };
}
