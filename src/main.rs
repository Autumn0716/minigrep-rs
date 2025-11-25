use std::env;
//args 只接受有效的 Unicode 值
use colored::Colorize;
use minigrep::Config;
use std::process;
fn main() {
    let _args: Vec<String> = env::args().collect::<Vec<String>>();
    // let config = parse_config(&args);
    // let config = Config::new(&args).unwrap_or_else(|err|{
    //     eprintln!("Problem parsing arguments:{}",err);
    //     process::exit(1);
    // });
    let config = Config::build().unwrap_or_else(|err| {
        eprintln!("Problem parsing arguments:{}", err);
        process::exit(1);
    });
    //unwrap_or_else能够处理 Result<T,E>类型的错误
    println!("Searching for {}", config.query.red());
    if let Err(e) = config.run() {
        eprintln!("Application error:{e}");
        process::exit(1);
    }
}
//test-driven development TDD
