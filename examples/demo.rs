use std::process::Command as StdCommand;
use elevated_command::Command;

fn main() {
    println!("=== Testing elevated_command with various argument types ===\n");

    // Test 1: Simple arguments
    println!("Test 1: Simple arguments");
    test_command(vec!["--help"]);
    
    // Test 2: Arguments with spaces (the problematic case)
    println!("\nTest 2: Arguments with spaces");
    test_command(vec!["-url", "https://example.com/path with spaces"]);
    
    // Test 3: Arguments with special characters
    println!("\nTest 3: Arguments with special characters");
    test_command(vec!["-key", "secret!@#$%^&*()"]);
    
    // Test 4: Arguments with quotes
    println!("\nTest 4: Arguments with quotes");
    test_command(vec!["-message", "He said \"hello\""]);
    
    // Test 5: Arguments with single quotes
    println!("\nTest 5: Arguments with single quotes");
    test_command(vec!["-path", "/Users/test/My App's Folder"]);
}

fn test_command(args: Vec<&str>) {
    // Use 'echo' to show what arguments are received
    let mut cmd = StdCommand::new("echo");
    cmd.arg("Arguments:");
    for arg in &args {
        cmd.arg(arg);
    }
    
    let elevated_cmd = Command::new(cmd);
    
    match elevated_cmd.output() {
        Ok(output) => {
            println!("  ✓ Status: {}", output.status);
            if !output.stdout.is_empty() {
                let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
                println!("  Output: {}", stdout);
            }
            if !output.stderr.is_empty() {
                println!("  Error: {}", String::from_utf8_lossy(&output.stderr));
            }
        }
        Err(e) => {
            println!("  ✗ Failed: {:?}", e);
        }
    }
}