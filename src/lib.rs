extern crate multiqueue2 as multiqueue;

use std::{
    fs::{self, File},
    io::{self, BufReader, BufWriter, Read, Write},
    path::PathBuf,
    process::exit,
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use multiqueue::{broadcast_queue, BroadcastReceiver};
use pathdiff::diff_paths;
use question::{Answer, Question};
use walkdir::WalkDir;

#[derive(Debug)]
struct Source {
    name: Option<String>,
    path: PathBuf,
}

fn get_input_paths(inputs: &Vec<String>) -> Vec<Source> {
    let mut file_list = vec![];

    for input in inputs {
        let path = PathBuf::from(&input);

        if path.is_file() {
            file_list.push(Source { name: None, path })
        } else {
            let mut root = path.clone();

            if let Some(r) = path.parent() {
                root = r.to_path_buf();
            };

            for entry in WalkDir::new(&path)
                .into_iter()
                .flat_map(|e| e.ok())
                .filter(|f| f.path().is_file())
            {
                let p = entry.path().to_path_buf();

                if let Some(r) = p.parent().and_then(|f| diff_paths(f, &root)) {
                    file_list.push(Source {
                        name: Some(r.display().to_string()),
                        path: entry.path().to_path_buf(),
                    });
                }
            }
        }
    }

    file_list
}

pub fn validate(inputs: &Vec<String>, outputs: &Vec<String>) -> Result<(), &'static str> {
    let mut override_all = false;
    let file_list = get_input_paths(inputs);
    let input_len = file_list.len();

    for input in file_list {
        let input_name = input.path.file_name().unwrap();

        for out in outputs {
            let out_path = PathBuf::from(&out);
            let mut path = out_path.clone();

            if out_path.is_dir() {
                if let Some(n) = input.name.clone() {
                    path = out_path.join(n.clone());
                };

                path = path.join(input_name);
            } else if input_len > 1 {
                return Err("Multiple input files to output file path, not supported!\nUse different output folders instead.");
            };

            if !override_all && path.is_file() {
                let answer = Question::new(
                    format!(
                        "Output file \"{}\" exists! Override all? [y/n]",
                        path.display()
                    )
                    .as_str(),
                )
                .yes_no()
                .until_acceptable()
                .ask();

                if answer == Some(Answer::NO) {
                    return Err("Cancel copy process.");
                }

                override_all = true
            }
        }
    }

    Ok(())
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

pub fn copy_process(
    inputs: Vec<String>,
    outputs: Vec<String>,
    show_progress: bool,
) -> Result<(), std::io::Error> {
    let file_list = get_input_paths(&inputs);

    for input in file_list {
        let mut buffer = [0; 65536];
        let mut counter = 0;
        let (send, recv) = broadcast_queue(512);
        let input_name = input.path.file_name().unwrap();
        let file_size = input.path.metadata()?.len();
        let file = File::open(&input.path)?;
        let mut input_reader = BufReader::new(file);
        let mut threads = vec![];

        for out in outputs.clone() {
            let out_path = PathBuf::from(&out);
            let mut path = out_path.clone();

            if out_path.is_dir() {
                if let Some(n) = input.name.clone() {
                    path = out_path.join(n.clone());
                    if !path.is_dir() {
                        fs::create_dir_all(&path)?;
                    }
                };

                path = path.join(input_name);
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
            let bytes_len = input_reader.read(&mut buffer[..])?;

            if bytes_len > 0 {
                let stamp = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or(Duration::from_millis(0))
                    .as_millis();
                counter += bytes_len as u64;
                size += bytes_len;

                if show_progress && stamp - start_time > 1000 {
                    start_time = stamp;

                    print!(
                        "\r  Progress: {}% rate: {1:.2}mb/s",
                        counter * 100 / file_size,
                        size as f64 / 1024.0 / 1024.0
                    );
                    io::stdout().flush()?;

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

    Ok(())
}
