extern crate multiqueue2 as multiqueue;

use std::{
    fs::File,
    io::{self, BufReader, BufWriter, Read, Write},
    path::{Path, PathBuf},
    process::exit,
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use clap::Parser;
use multiqueue::{broadcast_queue, BroadcastReceiver};
use question::{Answer, Question};

#[derive(Parser, Debug, Clone)]
#[clap(version,
    about = " Copy file to multiple targets .",
    long_about = None)]
struct Args {
    #[clap(
        short,
        long,
        help = "Input file, to for copy process.",
        multiple_values = true,
        required = true
    )]
    input: Vec<String>,

    #[clap(
        short,
        long,
        help = "Output file, can be used multiple times.",
        multiple_occurrences = true,
        required = true
    )]
    output: Vec<String>,

    #[clap(short, long, help = "Show progress.")]
    pub progress: bool,
}

fn writer(out: PathBuf, stream: BroadcastReceiver<(usize, [u8; 65536])>) {
    let file = match File::create(out) {
        Ok(f) => f,
        Err(e) => {
            println!("{e}");
            exit(1);
        }
    };
    let mut file_buffer = BufWriter::new(file);

    for val in stream {
        let (len, buf) = val;

        if let Err(e) = file_buffer.write(&buf[..len]) {
            println!("{e}");
        };
    }
}

fn validate(args: &Args) -> bool {
    for input in &args.input {
        if !Path::new(&input).is_file() {
            println!("Only existing files as input are supported!");

            return false;
        }
    }

    if args.input.len() > 1 {
        for out in &args.output {
            if Path::new(&out).is_file() {
                println!("Only one input and multiple output files, or multiple inputs and output folders supported!");

                return false;
            }
        }
    }

    for out in &args.output {
        let out_path = Path::new(&out);
        if out_path.is_file() {
            let answer = Question::new(
                format!("Output file \"{out}\" exists! Override all? [y/n]").as_str(),
            )
            .yes_no()
            .until_acceptable()
            .ask();

            if answer == Some(Answer::NO) {
                return false;
            }

            return true;
        }

        if out_path.is_dir() {
            for input in &args.input {
                let in_path = Path::new(input);
                let input_name = in_path.file_name().unwrap();
                let out_file = out_path.join(input_name);

                if out_file.is_file() {
                    let answer = Question::new(
                        format!(
                            "Output file \"{}\" exists! Override all? [y/n]",
                            out_file.display()
                        )
                        .as_str(),
                    )
                    .yes_no()
                    .until_acceptable()
                    .ask();

                    if answer == Some(Answer::NO) {
                        return false;
                    }
                }
            }
        }
    }

    true
}

fn main() {
    let args = Args::parse();

    if !validate(&args) {
        exit(1)
    }

    let inputs = args.input;

    for input in inputs {
        let mut buffer = [0; 65536];
        let mut counter = 0;
        let (send, recv) = broadcast_queue(92);
        let input_file = Path::new(&input);
        let input_name = input_file.file_name().unwrap();
        let file_size = match input_file.metadata() {
            Ok(s) => s.len(),
            Err(e) => {
                println!("{e:?}");
                exit(1);
            }
        };
        let file = match File::open(input_file) {
            Ok(f) => f,
            Err(e) => {
                println!("{e:?}");
                exit(1);
            }
        };
        let mut input_reader = BufReader::new(file);
        let mut threads = vec![];
        let outputs = args.output.clone();

        for out in outputs.clone() {
            let out_str = out.as_str();
            let out_path = PathBuf::from(out_str);
            let mut path = out_path.clone();

            if out_path.is_dir() {
                path = out_path.join(input_name);
            };
            let cur_recv = recv.add_stream();
            let t = thread::spawn(move || writer(path, cur_recv));

            threads.push(t);
        }

        recv.unsubscribe();
        let mut start_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::from_millis(0))
            .as_millis();
        let mut size = 0;

        loop {
            let bytes_len = match input_reader.read(&mut buffer[..]) {
                Ok(length) => length,
                Err(e) => {
                    println!("Reading error: {e:?}");
                    break;
                }
            };

            if bytes_len > 0 {
                let stamp = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or(Duration::from_millis(0))
                    .as_millis();
                counter += bytes_len as u64;
                size += bytes_len;

                if args.progress && stamp - start_time > 1000 {
                    start_time = stamp;

                    print!(
                        "\r  Progress: {}% rate: {1:.2}mb/s",
                        counter * 100 / file_size,
                        size as f64 / 1024.0 / 1024.0
                    );
                    io::stdout().flush().expect("Flushing STDOUT not possible");

                    size = 0;
                }

                loop {
                    if send.try_send((bytes_len, buffer)).is_ok() {
                        break;
                    }
                }
            } else {
                break;
            }
        }

        drop(send);

        for t in threads {
            if let Err(e) = t.join() {
                println!("{e:?}");
            };
        }
    }
}
