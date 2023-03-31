use std::io::prelude::*;
use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;
//use std::process::Command;
use std::sync::Arc;
use tokio::sync::Mutex;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LspService, Server};
//use tree_sitter::Point;
use clap::{arg, Arg, ArgAction, Command};
use std::collections::HashMap;
use tokio::net::TcpListener;
mod ast;
mod complete;
mod filewatcher;
mod formatting;
mod gammar;
mod jump;
mod languagerserver;
mod scansubs;
mod search;
mod utils;

/// Beckend
#[derive(Debug)]
struct Backend {
    /// client
    client: Client,
    /// Storage the message of buffers
    buffers: Arc<Mutex<HashMap<lsp_types::Url, String>>>,
}

#[tokio::main]
async fn main() {
    const VERSION: &str = env!("CARGO_PKG_VERSION");
    let matches = Command::new("neocmakelsp")
        .about("CMake LSP implementation based on Tower and Tree-sitter")
        .version(VERSION)
        .subcommand_required(true)
        .arg_required_else_help(true)
        .author("Cris")
        .subcommand(
            Command::new("stdio")
                .long_flag("stdio")
                .about("run with stdio"),
        )
        .subcommand(
            Command::new("tcp")
                .long_flag("tcp")
                .about("run with tcp")
                .arg(
                    Arg::new("port")
                        .long("port")
                        .short('P')
                        .help("listen to port"),
                ),
        )
        .subcommand(
            Command::new("search")
                .long_flag("search")
                .short_flag('S')
                .about("Search packages")
                .arg(arg!(<Package> ... "Packages"))
                .arg(
                    Arg::new("tojson")
                        .long("tojson")
                        .short('j')
                        .action(ArgAction::SetTrue)
                        .help("tojson"),
                ),
        )
        .subcommand(
            Command::new("format")
                .long_flag("format")
                .short_flag('F')
                .about("format the file")
                .arg(
                    arg!(<FoldPath> ... "glob pattern to format")
                        .value_parser(clap::value_parser!(String)),
                )
                .arg(
                    Arg::new("override")
                        .long("override")
                        .short('o')
                        .action(ArgAction::SetTrue)
                        .help("override"),
                ),
        )
        .subcommand(
            Command::new("tree")
                .long_flag("tree")
                .short_flag('T')
                .about("Tree the file")
                .arg(arg!(<PATH> ... "tree").value_parser(clap::value_parser!(PathBuf)))
                .arg(
                    Arg::new("tojson")
                        .long("tojson")
                        .short('j')
                        .action(ArgAction::SetTrue)
                        .help("tojson"),
                ),
        )
        .get_matches();
    match matches.subcommand() {
        Some(("search", sub_matches)) => {
            let packagename = sub_matches
                .get_one::<String>("Package")
                .expect("required one pacakge");
            if sub_matches.get_flag("tojson") {
                println!("{}", search::search_result_tojson(packagename));
            } else {
                println!("{}", search::search_result(packagename));
            }
        }
        Some(("format", sub_matches)) => {
            let path = sub_matches
                .get_one::<String>("FoldPath")
                .expect("Cannot get globpattern");
            let hasoverride = sub_matches.get_flag("override");
            let formatpattern = |pattern: &str| {
                for filepath in glob::glob(pattern)
                    .unwrap_or_else(|_| panic!("error pattern"))
                    .flatten()
                {
                    let mut file = match std::fs::OpenOptions::new()
                        .read(true)
                        .write(hasoverride)
                        .open(&filepath)
                    {
                        Ok(file) => file,
                        Err(e) => {
                            println!("cannot read file {} :{e}", filepath.display());
                            continue;
                        }
                    };
                    let mut buf = String::new();
                    file.read_to_string(&mut buf).unwrap();
                    let mut parse = tree_sitter::Parser::new();
                    parse.set_language(tree_sitter_cmake::language()).unwrap();
                    let tree = parse.parse(&buf, None).unwrap();
                    match formatting::get_format_cli(tree.root_node(), &buf) {
                        Some(context) => {
                            if hasoverride {
                                if let Err(e) = file.set_len(0) {
                                    println!("Cannot clear the file: {e}");
                                };
                                if let Err(e) = file.seek(std::io::SeekFrom::End(0)) {
                                    println!("Cannot jump to end: {e}");
                                };
                                let Ok(_) = file.write_all(context.as_bytes()) else {
                                    println!("cannot write in {}",filepath.display());
                                    continue;
                                };
                                let _ = file.flush();
                            } else {
                                println!("here");
                                println!("{context}")
                            }
                        }
                        None => {
                            println!("There is error in file");
                        }
                    }
                }
            };
            formatpattern(&format!("./{}/**/*.cmake", path));
            formatpattern(&format!("./{}/**/CMakeLists.txt", path));
        }
        Some(("tree", sub_matches)) => {
            let path = sub_matches
                .get_one::<PathBuf>("PATH")
                .expect("Cannot get path");
            match scansubs::get_treedir(path) {
                Some(tree) => {
                    if sub_matches.get_flag("tojson") {
                        println!("{}", serde_json::to_string(&tree).unwrap())
                    } else {
                        println!("{tree}")
                    }
                }
                None => println!("Nothing find"),
            };
        }
        Some(("stdio", _)) => {
            tracing_subscriber::fmt().init();
            let (stdin, stdout) = (tokio::io::stdin(), tokio::io::stdout());
            let (service, socket) = LspService::new(|client| Backend {
                client,
                buffers: Arc::new(Mutex::new(HashMap::new())),
            });
            Server::new(stdin, stdout, socket).serve(service).await;
        }
        Some(("tcp", sync_matches)) => {
            #[cfg(feature = "runtime-agnostic")]
            use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};
            tracing_subscriber::fmt().init();
            let stream = {
                if sync_matches.contains_id("port") {
                    let port = sync_matches.get_one::<String>("port").expect("error");
                    let port: u16 = port.parse().unwrap();
                    let listener = TcpListener::bind(SocketAddr::new(
                        std::net::IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
                        port,
                    ))
                    .await
                    .unwrap();
                    let (stream, _) = listener.accept().await.unwrap();
                    stream
                } else {
                    let listener = TcpListener::bind(SocketAddr::new(
                        std::net::IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
                        9257,
                    ))
                    .await
                    .unwrap();
                    let (stream, _) = listener.accept().await.unwrap();
                    stream
                }
            };

            let (read, write) = tokio::io::split(stream);
            #[cfg(feature = "runtime-agnostic")]
            let (read, write) = (read.compat(), write.compat_write());

            let (service, socket) = LspService::new(|client| Backend {
                client,
                buffers: Arc::new(Mutex::new(HashMap::new())),
            });
            Server::new(read, write, socket).serve(service).await;
        }
        _ => unimplemented!(),
    }
}
