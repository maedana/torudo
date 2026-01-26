use std::io::{self, Write};
use std::fs;
use std::path::Path;
use std::error::Error;

pub fn confirm_creation(item_type: &str, path: &str) -> io::Result<bool> {
    println!("{} does not exist: {}", item_type, path);
    print!("Create it? (y/N): ");
    io::stdout().flush()?;
    
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let input = input.trim().to_lowercase();
    
    Ok(input == "y" || input == "yes")
}

pub fn ensure_setup_exists(todotxt_dir: &str, todo_file: &str) -> Result<(), Box<dyn Error>> {
    // Check if todotxt directory exists
    if !Path::new(todotxt_dir).exists() {
        if !confirm_creation("todotxt directory", todotxt_dir)? {
            eprintln!("Creation of todotxt directory was rejected. Exiting application.");
            std::process::exit(1);
        }
        fs::create_dir_all(todotxt_dir)?;
        println!("Created todotxt directory: {}", todotxt_dir);
    }
    
    // Check if todo.txt exists
    if !Path::new(todo_file).exists() {
        if !confirm_creation("todo.txt", todo_file)? {
            eprintln!("Creation of todo.txt was rejected. Exiting application.");
            std::process::exit(1);
        }
        fs::write(todo_file, "")?;
        println!("Created todo.txt: {}", todo_file);
    }
    
    Ok(())
}

pub fn setup_debug_logging(todotxt_dir: &str) -> Result<(), Box<dyn Error>> {
    let debug_log_path = format!("{todotxt_dir}/debug.log");
    
    env_logger::Builder::from_default_env()
        .target(env_logger::Target::Pipe(Box::new(
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(debug_log_path)?
        )))
        .filter_level(log::LevelFilter::Debug)
        .format(|buf, record| {
            writeln!(buf, "[{}] {} - {}: {}",
                chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
                record.level(),
                record.target(),
                record.args()
            )
        })
        .init();
    
    Ok(())
}