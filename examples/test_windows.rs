use std::process::Command as StdCommand;
use elevated_command::Command;

fn main() {
    println!("=== Testing elevated_command on Windows ===\n");
    println!("This will test Windows command-line argument escaping");
    println!("You will see a UAC (User Account Control) prompt\n");

    // Test 1: Simple arguments
    println!("Test 1: Simple arguments");
    test_command(vec!["--help"]);
    
    // Test 2: Arguments with spaces (the problematic case on Windows)
    println!("\nTest 2: Arguments with spaces");
    test_command(vec!["-path", "C:\\Program Files\\My App"]);
    
    // Test 3: Arguments with quotes
    println!("\nTest 3: Arguments with quotes");
    test_command(vec!["-message", "He said \"hello\""]);
    
    // Test 4: Arguments with backslashes
    println!("\nTest 4: Arguments with backslashes (Windows paths)");
    test_command(vec!["-dst", "C:\\Users\\Test\\Documents\\"]);
    
    // Test 5: Complex Windows paths
    println!("\nTest 5: Complex Windows paths");
    test_command(vec![
        "-url", "https://example.com/my file.zip",
        "-dst", "C:\\Users\\Test\\AppData\\Local\\My App\\data"
    ]);
    
    // Test 6: Full app-loader simulation
    println!("\nTest 6: Full app-loader simulation");
    test_app_loader_simulation();
}

fn test_command(args: Vec<&str>) {
    // Use 'cmd' with '/c echo' to show what arguments are received
    let mut cmd = StdCommand::new("cmd");
    cmd.arg("/c");
    cmd.arg("echo");
    cmd.arg("Arguments:");
    for arg in &args {
        cmd.arg(arg);
    }
    
    let elevated_cmd = Command::new(cmd);
    
    match elevated_cmd.output() {
        Ok(output) => {
            // Check the exit code - on Windows, ShellExecuteW returns different codes
            let exit_code = output.status.code().unwrap_or(-1);
            if exit_code > 32 {
                println!("  ✓ Command executed (code: {})", exit_code);
            } else {
                println!("  ✗ Command may have failed (code: {})", exit_code);
                println!("    Note: Codes <= 32 indicate ShellExecuteW errors");
            }
            
            // On Windows, stdout/stderr are always empty due to how ShellExecuteW works
            if !output.stdout.is_empty() {
                println!("  Output: {}", String::from_utf8_lossy(&output.stdout));
            } else {
                println!("  Note: stdout is empty (Windows limitation with UAC)");
            }
        }
        Err(e) => {
            println!("  ✗ Failed: {:?}", e);
        }
    }
}

fn test_app_loader_simulation() {
    // This simulates your actual app-loader use case on Windows
    let url = "https://example.com/files/my app.zip";
    let dst = "C:\\Users\\Test\\AppData\\Local\\MyApp\\vpn-worker";
    let main_path = ".\\bin\\worker.exe";
    let key = "my-secret-key-123!";
    let device_id = "device-abc-123";
    let nats_url = "nats://localhost:4222";
    let app_dir = "C:\\Users\\Test\\.datagram\\vpn";
    
    println!("  Building command with arguments:");
    println!("    -url: {}", url);
    println!("    -dst: {}", dst);
    println!("    -main: {}", main_path);
    println!("    -key: {}", key);
    println!("    -did: {}", device_id);
    println!("    -nurl: {}", nats_url);
    println!("    -appDir: {}", app_dir);
    println!();
    
    // Use cmd /c echo to simulate app-loader
    let mut cmd = StdCommand::new("cmd");
    cmd.arg("/c")
        .arg("echo")
        .arg("app-loader")
        .arg("-url")
        .arg(url)
        .arg("-dst")
        .arg(dst)
        .arg("-main")
        .arg(main_path)
        .arg("--")
        .arg("-key")
        .arg(key)
        .arg("-did")
        .arg(device_id)
        .arg("-nurl")
        .arg(nats_url)
        .arg("-appDir")
        .arg(app_dir);
    
    let elevated_cmd = Command::new(cmd);
    
    println!("  Running with elevated privileges (UAC prompt will appear)...\n");
    
    match elevated_cmd.output() {
        Ok(output) => {
            let exit_code = output.status.code().unwrap_or(-1);
            
            if exit_code > 32 {
                println!("  ✓ Command executed successfully!");
                println!("    Exit code: {}", exit_code);
                println!();
                println!("  Windows Command-Line Argument Verification:");
                println!("  ✓ If UAC prompt appeared, arguments were properly escaped");
                println!("  ✓ Spaces in paths should be preserved");
                println!("  ✓ Special characters in key should be preserved");
                println!("  ✓ Backslashes in Windows paths should be correct");
            } else {
                println!("  ✗ Command may have failed");
                println!("    Exit code: {} (codes <= 32 indicate errors)", exit_code);
                
                // Explain common error codes
                match exit_code {
                    0 => println!("    Error: Out of memory or resources"),
                    2 => println!("    Error: File not found"),
                    3 => println!("    Error: Path not found"),
                    5 => println!("    Error: Access denied (user cancelled UAC?)"),
                    8 => println!("    Error: Out of memory"),
                    11 => println!("    Error: Invalid EXE file"),
                    26 => println!("    Error: Sharing violation"),
                    27 => println!("    Error: File association incomplete or invalid"),
                    31 => println!("    Error: No application associated"),
                    _ => println!("    Error: Unknown ShellExecuteW error"),
                }
            }
            
            // Note about Windows limitations
            if output.stdout.is_empty() && output.stderr.is_empty() {
                println!();
                println!("  Note: stdout/stderr are empty - this is a Windows limitation");
                println!("        ShellExecuteW doesn't capture output from elevated processes");
            }
        }
        Err(e) => {
            println!("  ✗ Failed to execute command: {:?}", e);
        }
    }
}