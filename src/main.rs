use kvarn::prelude::*;

pub mod hosts;

#[cfg_attr(feature = "mt", tokio::main)]
#[cfg_attr(not(feature = "mt"), tokio::main(flavor = "current_thread"))]
async fn main() {
    let env_log = env_logger::Env::new().filter_or("KVARN_LOG", "rustls=off,warn");
    env_logger::Builder::from_env(env_log).init();

    let (icelk_host, icelk_se) = hosts::icelk(hosts::icelk_extensions()).await;
    let kvarn_host = hosts::kvarn(hosts::kvarn_extensions());
    let kvarn_doc_host = hosts::kvarn_doc(hosts::kvarn_doc_extensions());
    let agde_host = hosts::agde(hosts::kvarn_extensions());
    let icelk_bitwarden_host = hosts::icelk_bitwarden(hosts::icelk_bitwarden_extensions());

    let host = std::env::args().nth(1);

    let mut hosts = match host.as_deref() {
        Some("--icelk") => HostCollection::builder().default(icelk_host),
        Some("--kvarn") => HostCollection::builder().default(kvarn_host),
        Some("--kvarn-doc") => HostCollection::builder().default(kvarn_doc_host),
        Some("--agde") => HostCollection::builder().default(agde_host),
        Some("--icelk-bitwarden") => HostCollection::builder().default(icelk_bitwarden_host),
        Some(_) => {
            error!("Unsupported host specifier");
            return;
        }
        _ => HostCollection::builder()
            .insert(icelk_host)
            .insert(kvarn_host)
            .insert(kvarn_doc_host)
            .insert(agde_host)
            .insert(icelk_bitwarden_host),
    };

    {
        hosts = hosts.insert(Host::unsecure(
            "mail.icelk.dev",
            "mail",
            Extensions::default(),
            host::Options::default(),
        ));
        hosts = hosts.insert(Host::unsecure(
            "mail.kvarn.org",
            "mail",
            Extensions::default(),
            host::Options::default(),
        ));
        hosts = hosts.insert(Host::unsecure(
            "mail.agde.dev",
            "mail",
            Extensions::default(),
            host::Options::default(),
        ));
    }

    let hosts = hosts.build();

    if hosts.get_host("icelk.dev").is_some() {
        icelk_se.watch("icelk.dev", Arc::clone(&hosts)).unwrap();
    }

    #[cfg(not(feature = "high_ports"))]
    let http_port = 80;
    #[cfg(all(not(feature = "high_ports"), feature = "https"))]
    let https_port = 443;
    #[cfg(feature = "high_ports")]
    let http_port = 8080;
    #[cfg(all(feature = "high_ports", feature = "https"))]
    let https_port = 8443;

    let mut ports = RunConfig::new();

    ports = ports.bind(kvarn::PortDescriptor::unsecure(
        http_port,
        Arc::clone(&hosts),
    ));

    #[cfg(feature = "https")]
    if hosts.has_secure() {
        ports = ports.bind(kvarn::PortDescriptor::new(https_port, Arc::clone(&hosts)));
    }

    let shutdown_manager = ports.execute().await;

    #[cfg(not(feature = "interactive"))]
    shutdown_manager.wait().await;

    #[cfg(feature = "interactive")]
    {
        let waiter = shutdown_manager.clone();
        // Exit the application on shutdown.
        tokio::spawn(async move {
            waiter.wait().await;
            info!("Shutdown complete. Exiting binary.");
            std::process::exit(0);
        });

        // Create a thread to .wait the child. This ensures the child is killed when ctrl+c and
        // other methods of exiting.
        std::thread::spawn(|| {
            // Start `kvarn-chute`
            static CHUTE_COMMAND: &str = "chute";
            match std::process::Command::new(CHUTE_COMMAND).arg("../").spawn() {
                Ok(mut child) => {
                    println!("Successfully started '{}'.", CHUTE_COMMAND);
                    child.wait().unwrap();
                }
                Err(_) => {
                    eprintln!("Failed to start '{}'.", CHUTE_COMMAND);
                }
            }
        });

        let sm = Arc::clone(&shutdown_manager);
        let thread = std::thread::spawn(move || {
            use futures::executor::block_on;
            use std::io::{prelude::*, stdin};

            // Commands in console
            for line in stdin().lock().lines().flatten() {
                let mut words = line.split(' ');
                if let Some(command) = words.next() {
                    match command {
                        "fcc" => {
                            // File cache clear
                            match block_on(
                                hosts.clear_file_in_cache(&Path::new(words.next().unwrap_or(""))),
                            ) {
                                true => println!("Removed item from cache!"),
                                false => println!("No item to remove"),
                            }
                        }
                        "rcc" => {
                            // Response cache clear
                            let host = match words.next() {
                                Some(word) => word,
                                None => {
                                    println!("Please enter a host to clear cache in.");
                                    continue;
                                }
                            };
                            let uri = match Uri::builder()
                                .path_and_query(words.next().unwrap_or(""))
                                .build()
                            {
                                Ok(uri) => uri,
                                Err(..) => {
                                    eprintln!("Failed to format path");
                                    continue;
                                }
                            };
                            let (cleared, found) = block_on(hosts.clear_page(host, &uri));

                            if !found {
                                println!("Did not found host to remove cached item from. Use 'default' or an empty string (e.g. '') for the default host.");
                            } else if !cleared {
                                println!("Did not remove any cached response.");
                            } else {
                                println!("Cleared a cached response.");
                            }
                        }
                        "cfc" => {
                            block_on(hosts.clear_file_caches());
                            println!("Cleared file system cache!");
                        }
                        "crc" => {
                            block_on(hosts.clear_response_caches());
                            println!("Cleared whole response cache.",);
                        }
                        "cc" => {
                            let hosts = hosts.clone();
                            block_on(async move {
                                hosts.clear_response_caches().await;
                                hosts.clear_file_caches().await
                            });
                            println!("Cleared all caches!");
                        }
                        "shutdown" | "sd" => {
                            sm.shutdown();
                        }
                        _ => {
                            eprintln!("Unknown command!");
                        }
                    }
                }
            }
        });
        thread.join().unwrap();

        shutdown_manager.shutdown();
        shutdown_manager.wait().await;
    }
}
