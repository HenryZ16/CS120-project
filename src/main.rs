mod acoustic_modem;
mod asio_stream;
mod pa0;
mod pa1;
mod tests;
mod utils;

fn help() {
    println!("Usage: ./CS120-project.exe [options]");
    println!("Options:");
    println!("  -h, --help: Show this help message");
    println!("  -p=N, --pa=N: Select PA N to demonstrate");
    println!("  -o, --objective=N: Select an objective N in a specified PA to demonstrate. If no PA specified, this will be ignored.");
    println!("  -t=<str>, --type=<str>: Additional type for the selected PA: send, send_file");
    println!("  -d, -device: Show available ASIO devices");
    println!("  -g[=N], --generate[=N]: Generate a random data file with N (default 10000) bits");
}

fn arg_parser(args: Vec<String>) -> Option<(i32, i32, String)> {
    if args.len() == 0 {
        help();
        std::process::exit(0);
    }

    let mut pa: i32 = 0;
    let mut objective: i32 = 0;
    let mut additional_type: String = String::new();

    for arg in args {
        if arg == "-h" || arg == "--help" {
            help();
            std::process::exit(0);
        } else if arg.starts_with("-p=") || arg.starts_with("--pa=") {
            let pa_str = arg.split("=").collect::<Vec<&str>>()[1];
            pa = match pa_str.parse::<i32>() {
                Ok(n) => n,
                Err(_) => {
                    println!("Invalid PA number");
                    std::process::exit(1);
                }
            };
        } else if arg.starts_with("-o=") || arg.starts_with("--objective=") {
            let objective_str = arg.split("=").collect::<Vec<&str>>()[1];
            objective = match objective_str.parse::<i32>() {
                Ok(n) => n,
                Err(_) => {
                    println!("Invalid objective number");
                    std::process::exit(1);
                }
            };
        } else if arg == "-d" || arg == "--device" {
            asio_stream::show_devices();
            return None;
        } else if arg.starts_with("-g") || arg.starts_with("--generate") {
            let len = match arg.split("=").collect::<Vec<&str>>().get(1) {
                Some(n) => match n.parse::<usize>() {
                    Ok(n) => n,
                    Err(_) => 10000,
                },
                None => 10000,
            };
            utils::gen_random_data_file(len);
            return None;
        } else if arg.starts_with("-t=") || arg.starts_with("--type=") {
            additional_type = arg.split("=").collect::<Vec<&str>>()[1].to_string();
        } else {
            println!("Invalid argument");
            std::process::exit(1);
        }
    }

    return Some((pa, objective, additional_type));
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match arg_parser(args) {
        Some((0, 0, _)) => {
            println!("PA 0 selected.");
            pa0::pa0(0).await.unwrap();
        }
        Some((0, n, _)) => {
            println!("PA 0 selected with objective {}.", n);
            match pa0::pa0(n).await {
                Ok(_) => {}
                Err(e) => {
                    println!("Error: {}", e);
                }
            }
        }
        Some((1, 0, additional_type)) => {
            println!("PA 1 selected.");
            pa1::pa1(0, &additional_type).await.unwrap();
        }
        Some((1, n, additional_type)) => {
            println!("PA 1 selected with objective {}.", n);
            match pa1::pa1(n, &additional_type).await {
                Ok(_) => {}
                Err(e) => {
                    println!("Error: {}", e);
                }
            }
        }
        Some((_, _, _)) => {
            println!("Invalid PA number");
            std::process::exit(1);
        }
        None => {}
    }
}
