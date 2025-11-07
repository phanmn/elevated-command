use std::process::Command as StdCommand;
use elevated_command::{Command, CommandEvent};
// use std::array::repeat;

fn main() {
    println!("=== Testing elevated_command spawn() with real-time output ===\n");
    
    // Example 1: Simple command with real-time output
    println!("Test 1: Echo command with delay (to see streaming)");
    test_spawn_simple();
    
    // println!("\n" + "=".repeat(60) + "\n");
    
    // Example 2: Command that produces lots of output
    println!("Test 2: Command with continuous output");
    test_spawn_continuous();
    
    // println!("\n" + "=".repeat(60) + "\n");
    
    // Example 3: App-loader simulation
    println!("Test 3: App-loader simulation with streaming");
    test_spawn_app_loader();
}

fn test_spawn_simple() {
    // Simple bash script that outputs slowly
    let script = r#"
        echo "Starting process..."
        sleep 1
        echo "Step 1 complete"
        sleep 1
        echo "Step 2 complete"
        sleep 1
        echo "All done!"
    "#;
    
    let mut cmd = StdCommand::new("bash");
    cmd.arg("-c").arg(script);
    
    let elevated_cmd = Command::new(cmd);
    
    println!("Starting elevated command (you'll be prompted for password)...\n");
    
    match elevated_cmd.spawn() {
        Ok((rx, _child)) => {
            println!("Command spawned! Streaming output:\n");
            
            while let Ok(event) = rx.recv() {
                match event {
                    CommandEvent::Stdout(data) => {
                        let text = String::from_utf8_lossy(&data);
                        print!("  [stdout] {}", text);
                    }
                    CommandEvent::Stderr(data) => {
                        let text = String::from_utf8_lossy(&data);
                        eprint!("  [stderr] {}", text);
                    }
                    CommandEvent::Terminated { code } => {
                        println!("\n✓ Process terminated with exit code: {:?}", code);
                        break;
                    }
                    CommandEvent::Error(err) => {
                        eprintln!("\n✗ Error: {}", err);
                        break;
                    }
                }
            }
        }
        Err(e) => {
            eprintln!("✗ Failed to spawn: {:?}", e);
        }
    }
}

fn test_spawn_continuous() {
    // Command that outputs numbers continuously
    let script = r#"
        for i in {1..10}; do
            echo "Output line $i"
            sleep 0.2
        done
    "#;
    
    let mut cmd = StdCommand::new("bash");
    cmd.arg("-c").arg(script);
    
    let elevated_cmd = Command::new(cmd);
    
    println!("Starting continuous output command...\n");
    
    match elevated_cmd.spawn() {
        Ok((rx, _child)) => {
            let mut line_count = 0;
            
            while let Ok(event) = rx.recv() {
                match event {
                    CommandEvent::Stdout(data) => {
                        let text = String::from_utf8_lossy(&data);
                        for line in text.lines() {
                            if !line.is_empty() {
                                line_count += 1;
                                println!("  [{:2}] {}", line_count, line);
                            }
                        }
                    }
                    CommandEvent::Stderr(data) => {
                        let text = String::from_utf8_lossy(&data);
                        eprint!("  [ERR] {}", text);
                    }
                    CommandEvent::Terminated { code } => {
                        println!("\n✓ Received {} lines of output", line_count);
                        println!("✓ Process terminated with code: {:?}", code);
                        break;
                    }
                    CommandEvent::Error(err) => {
                        eprintln!("\n✗ Error: {}", err);
                        break;
                    }
                }
            }
        }
        Err(e) => {
            eprintln!("✗ Failed to spawn: {:?}", e);
        }
    }
}

fn test_spawn_app_loader() {
    // Simulate your app-loader with a script that shows progress
    let script = r#"
        echo "app-loader starting..."
        echo "  URL: https://example.com/app.zip"
        echo "  Destination: /tmp/app-data"
        echo ""
        
        echo "[1/4] Downloading..."
        sleep 1
        echo "  Downloaded 1MB / 10MB"
        sleep 1
        echo "  Downloaded 5MB / 10MB"
        sleep 1
        echo "  Downloaded 10MB / 10MB"
        echo "  ✓ Download complete"
        echo ""
        
        echo "[2/4] Extracting..."
        sleep 1
        echo "  Extracting files..."
        echo "  ✓ Extraction complete"
        echo ""
        
        echo "[3/4] Verifying..."
        sleep 0.5
        echo "  ✓ Verification passed"
        echo ""
        
        echo "[4/4] Starting worker..."
        sleep 0.5
        echo "  ✓ Worker started"
        echo ""
        
        echo "app-loader completed successfully!"
    "#;
    
    let mut cmd = StdCommand::new("bash");
    cmd.arg("-c").arg(script);
    
    // Add environment variables like your real app-loader
    cmd.env("TOP_PARENT_ID", "12345")
       .env("DG_DEVICE_ID", "test-device")
       .env("DG_NATS_URL", "nats://localhost:4222");
    
    let elevated_cmd = Command::new(cmd);
    
    println!("Simulating app-loader with elevated privileges...\n");
    
    match elevated_cmd.spawn() {
        Ok((rx, _child)) => {
            while let Ok(event) = rx.recv() {
                match event {
                    CommandEvent::Stdout(data) => {
                        let text = String::from_utf8_lossy(&data);
                        print!("{}", text);
                    }
                    CommandEvent::Stderr(data) => {
                        let text = String::from_utf8_lossy(&data);
                        eprint!("ERROR: {}", text);
                    }
                    CommandEvent::Terminated { code } => {
                        println!("\n✓ App-loader finished with code: {:?}", code);
                        break;
                    }
                    CommandEvent::Error(err) => {
                        eprintln!("\n✗ App-loader error: {}", err);
                        break;
                    }
                }
            }
        }
        Err(e) => {
            eprintln!("✗ Failed to spawn app-loader: {:?}", e);
        }
    }
}
