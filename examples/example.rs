use fwupd_dbus::{Client, Signal};
use std::{
    error::Error,
    process,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};

fn main() {
    if let Err(why) = main_() {
        let mut error = format!("error: {}", why);
        let mut cause = why.source();
        while let Some(why) = cause {
            error.push_str(&format!("\n    caused by: {}", why));
            cause = why.source();
        }

        eprintln!("{}", error);
        process::exit(1);
    }
}

fn main_() -> Result<(), Box<dyn Error>> {
    // Atomic value used to stop the background thread.
    let cancellable = Arc::new(AtomicBool::new(true));

    // Begin listening to signals in the background
    listen_in_background(cancellable.clone());

    // Create a new dbus client connection.
    let fwupd = &Client::new()?;

    println!("Version: {}", fwupd.daemon_version()?);
    println!("Status: {:?}", fwupd.status()?);
    println!("Tainted: {}", fwupd.tainted()?);
    if let Ok(percent) = fwupd.percentage() {
        println!("Percentage; {}", percent);
    }

    // Fetch a list of supported devices.
    for device in fwupd.devices()? {
        println!("Device: {:#?}", device);

        if device.is_updateable() {
            if let Ok(upgrades) = fwupd.upgrades(&device) {
                println!("  upgrades found");
                for upgrade in upgrades {
                    println!("{:#?}", upgrade);
                }
            } else {
                println!("  no updates available");
            }

            if let Ok(downgrades) = fwupd.downgrades(&device) {
                println!("  downgrades found");
                for downgrade in downgrades {
                    println!("{:#?}", downgrade);
                }
            }

            if let Ok(releases) = fwupd.releases(&device) {
                println!("   releases found");
                for release in releases {
                    println!("{:#?}", release);
                }
            }
        } else {
            println!("  device not updateable");
        }
    }

    // Fetch a list of remotes, and update them.
    for remote in fwupd.remotes()? {
        println!("{:#?}", remote);

        remote.update_metadata(fwupd)?;
    }

    loop {
        std::thread::sleep(Duration::from_secs(1));
    }

    // Stop listening to signals in the background.
    cancellable.store(true, Ordering::SeqCst);

    Ok(())
}

fn listen_in_background(cancellable: Arc<AtomicBool>) {
    thread::spawn(move || {
        if let Ok(fwupd) = Client::new() {
            // Listen for signals received by the daemon.
            let signals = fwupd.listen_signals(cancellable).unwrap();
            for signal in signals {
                match signal {
                    Signal::Changed => {
                        println!("changed");
                    }
                    Signal::DeviceAdded(device) => {
                        println!("device added: {:?}", device);
                    }
                    Signal::DeviceChanged(device) => {
                        println!("device changed: {:?}", device);
                    }
                    Signal::DeviceRemoved(device) => {
                        println!("device added: {:?}", device);
                    }
                    Signal::DeviceRequest(request) => {
                        println!("device request: {:?}", request);
                    }
                    Signal::PropertiesChanged { interface, changed, invalidated } => {
                        println!(
                            "Properties of {} changed:\n changed: {:?}\n invalidated: {:?}",
                            interface, changed, invalidated
                        );
                    }
                }
            }
        }

        eprintln!("STOPPED LISTENING");
    });
}
