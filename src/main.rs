use buaa_api::Session;
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use tokio::time::{self, Duration};

use std::fs::{File, OpenOptions};
use std::io::Write;

#[derive(Debug, Parser)]
#[command(
    version = "0.1.0",
    about = "A cli for BUAA Boya",
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Login to Boya.
    /// Username and Password can be saved in the configuration file, and you can also specify them here.
    /// Notice: Token is easy to expire, so you may need to login again.
    Login {
        #[arg(short, long)]
        username: Option<String>,
        #[arg(short, long)]
        password: Option<String>,
    },
    /// Query courses and select a course by ID
    Query {
        #[arg(short, long)]
        /// By default, only optional courses are displayed, and enable this to display all courses
        all: bool,
    },
    /// Drop a course by ID
    Drop {
        #[arg(short, long)]
        id: u32,
    }
}

#[derive(Debug, Default, Deserialize, Serialize)]
struct Config {
    username: String,
    password: String,
    token: String,
}

fn main() {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open("buaa-boya-config.json")
        .unwrap();
    let mut config = match serde_json::from_reader::<File, Config>(file){
        Ok(config) => config,
        Err(_) => Config::default(),
    };
    let mut session = Session::new_in_file("buaa-boya-cookie.json");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let cli = Cli::parse();
    match cli.command {
        Commands::Login { username, password } => {
            if let Some(username) = username {
                config.username = username;
            }
            if let Some(password) = password {
                config.password = password;
            }
            runtime.block_on(async {
                match session.sso_login(&config.username, &config.password).await {
                    Ok(_) => println!("[Info]: SSO Login successfully"),
                    Err(e) => eprintln!("[Info]: Boya Login failed: {:?}", e),
                }
                let token = match session.bykc_login().await {
                    Ok(s) => {
                        println!("[Info]: SSO Login successfully");
                        s
                    },
                    Err(e) => {
                        eprintln!("[Info]: Boya Login failed: {:?}", e);
                        return;
                    },
                };
                config.token = token;
            });
        },
        Commands::Query { all } => {
            runtime.block_on(async {
                let courses = match session.bykc_query_course(&config.token).await {
                    Ok(courses) => courses,
                    Err(e) => {
                        eprintln!("[Info]: Query failed: {:?}", e);
                        eprintln!("[Info]: Consider login again");
                        return;
                    },
                };
                // 默认显示过滤过的可选课程
                if all {
                    println!("{}", buaa_api::utils::table(&courses));
                } else {
                    let time = buaa_api::utils::get_primitive_time();
                    let courses = courses.iter()
                        .filter(|course| course.capacity.current < course.capacity.max && course.time.select_end > time)
                        .collect::<Vec<_>>();
                    println!("{}", buaa_api::utils::table(&courses));
                }

                // 输入 ID 选择课程
                print!("Type ID to select course: ");
                std::io::stdout().flush().unwrap();
                let mut id = String::new();
                std::io::stdin().read_line(&mut id).unwrap();

                let id: u32 = match id.trim().parse() {
                    Ok(num) => num,
                    Err(_) => {
                        eprintln!("[Error]: Invalid ID");
                        return;
                    }
                };

                let course = courses.iter().find(|course| course.id == id).unwrap();
                let now = buaa_api::utils::get_primitive_time();
                let duration = course.time.select_start - now;
                let second = duration.whole_seconds() + 1;
                // 如果时间大于零那么就等待
                if second > 0 {
                    let duration = Duration::from_secs(second as u64);
                    println!("[Info]: Waiting for {} seconds", second);
                    time::sleep(duration).await;
                    // 可能 token 已经过期重新获取一下
                    let token = match session.bykc_login().await {
                        Ok(s) => {
                            println!("[Info]: SSO Login successfully");
                            s
                        },
                        Err(e) => {
                            eprintln!("[Info]: Boya Login failed: {:?}", e);
                            return;
                        },
                    };
                    config.token = token;
                }
                match session.bykc_select_course(id, &config.token).await {
                    Ok(_) => println!("[Info]: Select successfully"),
                    Err(e) => {
                        eprintln!("[Info]: Select failed: {:?}", e);
                        eprintln!("[Info]: Consider login again");
                    },
                }
            });
        },
        Commands::Drop { id } => {
            runtime.block_on(async {
                match session.bykc_drop_course(id, &config.token).await {
                    Ok(_) => println!("[Info]: Drop successfully"),
                    Err(e) => {
                        eprintln!("[Info]: Drop failed: {:?}", e);
                        eprintln!("[Info]: Consider login again");
                    },
                }
            });
        }
    }
    session.save();
    let file = OpenOptions::new()
        .write(true)
        .open("buaa-boya-config.json")
        .unwrap();
    serde_json::to_writer(file, &config).unwrap();
}