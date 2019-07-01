use fwupd_dbus::{Client, Signal};
use std::{
    error::Error,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
};

fn main() -> Result<(), Box<dyn Error>> {
    // Atomic value used to stop the background thread.
    let cancellable = Arc::new(AtomicBool::new(false));

    // Begin listening to signals in the background
    listen_in_background(cancellable.clone());

    // Create a new dbus client connection.
    let client = Client::new()?;

    // Fetch a list of supported devices.
    for device in client.get_devices()? {
        println!("Device: {} {}", device.vendor, device.name);

        if device.is_updateable() {
            if let Ok(upgrades) = device.upgrades(&client) {
                println!("  upgrades found");
                for upgrade in upgrades {
                    for (key, value) in upgrade {
                        println!("    {}: {:?}", key, value);
                    }
                }
            } else {
                println!("  no updates available");
            }

            if let Ok(downgrades) = device.downgrades(&client) {
                println!("  downgrades found");
                for downgrade in downgrades {
                    for (key, value) in downgrade {
                        println!("    {}: {:?}", key, value);
                    }
                }
            }

            if let Ok(releases) = device.releases(&client) {
                println!("   releases found");
                for release in releases {
                    for (key, value) in release {
                        println!("    {}: {:?}", key, value);
                    }
                }
            }
        } else {
            println!("  device not updateable");
        }
    }

    // Stop listening to signals in the background.
    cancellable.store(true, Ordering::SeqCst);

    Ok(())
}

fn listen_in_background(cancellable: Arc<AtomicBool>) {
    thread::spawn(move || {
        if let Ok(client) = Client::new() {
            // Listen for signals received by the daemon.
            for signal in client.listen_signals(cancellable) {
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
                }
            }
        }
    });
}
