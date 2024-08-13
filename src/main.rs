mod acoustic_modem;
mod asio_stream;
mod pa0;
mod pa1;
mod symrs;
mod tests;

fn help() {
    println!("Usage: ./CS120-project.exe [options]");
    println!("Options:");
    println!("  -h, --help: Show this help message");
    println!("  -p=N, --pa=N: Select PA N to demonstrate");
    println!("  -o, --objective=N: Select an objective N in a specified PA to demonstrate. If no PA specified, this will be ignored.");
}

fn arg_parser(args: Vec<String>) -> (u32, u32) {
    if args.len() == 0 {
        help();
        std::process::exit(0);
    }

    let mut pa = 0;
    let mut objective = 0;

    for arg in args {
        if arg == "-h" || arg == "--help" {
            help();
            std::process::exit(0);
        } else if arg.starts_with("-p=") || arg.starts_with("--pa=") {
            let pa_str = arg.split("=").collect::<Vec<&str>>()[1];
            pa = match pa_str.parse::<u32>() {
                Ok(n) => n,
                Err(_) => {
                    println!("Invalid PA number");
                    std::process::exit(1);
                }
            };
        } else if arg.starts_with("-o=") || arg.starts_with("--objective=") {
            let objective_str = arg.split("=").collect::<Vec<&str>>()[1];
            objective = match objective_str.parse::<u32>() {
                Ok(n) => n,
                Err(_) => {
                    println!("Invalid objective number");
                    std::process::exit(1);
                }
            };
        }
    }

    return (pa, objective);
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match arg_parser(args) {
        (0, 0) => {
            println!("PA 0 selected.");
            pa0::pa0(0).await.unwrap();
        }
        (0, n) => {
            println!("PA 0 selected with objective {}.", n);
            match pa0::pa0(n).await {
                Ok(_) => {}
                Err(e) => {
                    println!("Error: {}", e);
                }
            }
        }
        (1, 0) => {
            println!("PA 1 selected.");
            pa1::pa1(0).await.unwrap();
        }
        (1, n) => {
            println!("PA 1 selected with objective {}.", n);
            match pa1::pa1(n).await {
                Ok(_) => {}
                Err(e) => {
                    println!("Error: {}", e);
                }
            }
        }
        _ => {
            println!("Invalid selection");
            std::process::exit(1);
        }
    }
}
