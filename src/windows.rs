/*---------------------------------------------------------------------------------------------
 *  This is an ALTERNATIVE windows.rs implementation with OUTPUT CAPTURE
 *  
 *  Replace the original windows.rs with this file to enable stdout/stderr capture on Windows.
 *  
 *  This version properly waits for elevated processes using ShellExecuteExW with
 *  SEE_MASK_NOCLOSEPROCESS and WaitForSingleObject, ensuring reliable synchronization.
 *--------------------------------------------------------------------------------------------*/

use crate::Command;
use crate::CommandChild;
use crate::CommandEvent;
use anyhow::Result;
use std::env;
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::mem;
use std::os::windows::process::ExitStatusExt;
use std::path::PathBuf;
use std::process::{Output, ExitStatus};
use std::sync::mpsc::{channel, Sender, Receiver};
use std::thread;
use std::time::Duration;
use winapi::shared::minwindef::{DWORD, LPVOID};
use winapi::um::processthreadsapi::{GetCurrentProcess, OpenProcessToken};
use winapi::um::securitybaseapi::GetTokenInformation;
use winapi::um::winnt::{HANDLE, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY};
use windows::core::{HSTRING, PCWSTR, w};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Shell::{ShellExecuteW, ShellExecuteExW, SHELLEXECUTEINFOW};
use windows::Win32::UI::WindowsAndMessaging::SW_HIDE;
use windows::Win32::UI::Shell::{SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS};
use windows::Win32::System::Threading::{WaitForSingleObject, GetExitCodeProcess, INFINITE};


/// The implementation of state check and elevated executing varies on each platform
impl Command {
    /// Check the state the current program running
    /// 
    /// Return `true` if the program is running as root, otherwise false
    /// 
    /// # Examples
    ///
    /// ```no_run
    /// use elevated_command::Command;
    ///
    /// fn main() {
    ///     let is_elevated = Command::is_elevated();
    ///
    /// }
    /// ```
    pub fn is_elevated() -> bool {
        // Thanks to https://stackoverflow.com/a/8196291
        unsafe {
            let mut current_token_ptr: HANDLE = mem::zeroed();
            let mut token_elevation: TOKEN_ELEVATION = mem::zeroed();
            let token_elevation_type_ptr: *mut TOKEN_ELEVATION = &mut token_elevation;
            let mut size: DWORD = 0;
    
            let result = OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut current_token_ptr);
    
            if result != 0 {
                let result = GetTokenInformation(
                    current_token_ptr,
                    TokenElevation,
                    token_elevation_type_ptr as LPVOID,
                    mem::size_of::<winapi::um::winnt::TOKEN_ELEVATION_TYPE>() as u32,
                    &mut size,
                );
                if result != 0 {
                    return token_elevation.TokenIsElevated != 0;
                }
            }
        }
        false
    }

    /// Prompting the user with a graphical OS dialog for the root password, 
    /// excuting the command with escalated privileges, and return the output
    /// 
    /// This version properly waits for the elevated process to complete using
    /// ShellExecuteExW with SEE_MASK_NOCLOSEPROCESS and WaitForSingleObject.
    /// 
    /// This version CAPTURES stdout and stderr by writing them to temporary files.
    /// The function blocks until the elevated process completes.
    /// 
    /// # Examples
    ///
    /// ```no_run
    /// use elevated_command::Command;
    /// use std::process::Command as StdCommand;
    ///
    /// fn main() {
    ///     let mut cmd = StdCommand::new("path to the application");
    ///     let elevated_cmd = Command::new(cmd);
    ///     let output = elevated_cmd.output().unwrap();
    /// }
    /// ```
    pub fn output(&self) -> Result<Output> {
        // Helper function to escape Windows command-line arguments
        fn windows_escape_arg(arg: &str) -> String {
            if arg.is_empty() {
                return "\"\"".to_string();
            }
            
            if !arg.chars().any(|c| c == ' ' || c == '\t' || c == '\n' || c == '\"' || c == '\\') {
                return arg.to_string();
            }
            
            let mut result = String::from("\"");
            let mut num_backslashes = 0;
            
            for c in arg.chars() {
                match c {
                    '\\' => {
                        num_backslashes += 1;
                    }
                    '"' => {
                        result.push_str(&"\\".repeat(num_backslashes * 2 + 1));
                        result.push('"');
                        num_backslashes = 0;
                    }
                    _ => {
                        if num_backslashes > 0 {
                            result.push_str(&"\\".repeat(num_backslashes));
                            num_backslashes = 0;
                        }
                        result.push(c);
                    }
                }
            }
            
            result.push_str(&"\\".repeat(num_backslashes * 2));
            result.push('"');
            result
        }

        // Create temporary files for output capture
        let temp_dir = env::temp_dir();
        let process_id = std::process::id();
        let stdout_file = temp_dir.join(format!("elevated_cmd_stdout_{}.txt", process_id));
        let stderr_file = temp_dir.join(format!("elevated_cmd_stderr_{}.txt", process_id));
        let exitcode_file = temp_dir.join(format!("elevated_cmd_exitcode_{}.txt", process_id));
        
        // Build a wrapper batch script that captures output
        let wrapper_script = temp_dir.join(format!("elevated_cmd_wrapper_{}.bat", process_id));
        
        let mut script_content = String::new();
        script_content.push_str("@echo off\r\n");
        
        // Add environment variables
        for (k, v) in self.cmd.get_envs() {
            if let Some(value) = v {
                script_content.push_str(&format!("set {}={}\r\n",
                    k.to_str().unwrap(),
                    value.to_str().unwrap()
                ));
            }
        }
        
        // Build the command with escaped arguments
        let program = windows_escape_arg(self.cmd.get_program().to_str().unwrap());
        let args = self.cmd.get_args()
            .map(|c| windows_escape_arg(c.to_str().unwrap()))
            .collect::<Vec<String>>();
        
        // Execute command and redirect output to files
        script_content.push_str(&program);
        if !args.is_empty() {
            script_content.push_str(&format!(" {}", args.join(" ")));
        }
        script_content.push_str(&format!(" 1>\"{}\" 2>\"{}\"\r\n", 
            stdout_file.to_str().unwrap(),
            stderr_file.to_str().unwrap()
        ));
        
        // Save the actual exit code
        script_content.push_str(&format!("echo %ERRORLEVEL%>\"{}\"", exitcode_file.to_str().unwrap()));
        
        // Write the wrapper script
        fs::write(&wrapper_script, script_content.as_bytes())?;

        // Execute the wrapper script with elevation using ShellExecuteExW
        let verb = w!("runas");
        let file = HSTRING::from(wrapper_script.to_str().unwrap());
        let params = HSTRING::new();
        
        let mut sei = SHELLEXECUTEINFOW {
            cbSize: mem::size_of::<SHELLEXECUTEINFOW>() as u32,
            fMask: SEE_MASK_NOASYNC | SEE_MASK_NOCLOSEPROCESS,
            hwnd: HWND(0),
            lpVerb: PCWSTR(verb.as_ptr()),
            lpFile: PCWSTR(file.as_ptr()),
            lpParameters: PCWSTR(params.as_ptr()),
            lpDirectory: PCWSTR::null(),
            nShow: SW_HIDE.0,
            hInstApp: Default::default(),
            lpIDList: std::ptr::null_mut(),
            lpClass: PCWSTR::null(),
            hkeyClass: Default::default(),
            dwHotKey: 0,
            Anonymous: Default::default(),
            hProcess: Default::default(),
        };

        let success = unsafe { ShellExecuteExW(&mut sei) };
        
        if success.is_err() || sei.hProcess.is_invalid() {
            // Clean up temporary files on failure
            let _ = fs::remove_file(&wrapper_script);
            let _ = fs::remove_file(&stdout_file);
            let _ = fs::remove_file(&stderr_file);
            let _ = fs::remove_file(&exitcode_file);
            return Err(anyhow::anyhow!("Failed to execute elevated command"));
        }

        // Wait for the elevated process to complete
        unsafe { 
            WaitForSingleObject(sei.hProcess, INFINITE);
        }

        // Get the exit code from the process (this is cmd.exe's return, not the script's)
        let mut shell_exit_code: u32 = 0;
        unsafe {
           let _ = GetExitCodeProcess(sei.hProcess, &mut shell_exit_code);
        }

        // Read the actual command exit code from the file
        let actual_exit_code = if let Ok(exitcode_data) = fs::read(&exitcode_file) {
            if let Ok(code_str) = String::from_utf8(exitcode_data) {
                code_str.trim().parse::<i32>().unwrap_or(0)
            } else {
                0
            }
        } else {
            0
        };
        
        // Read output files
        let stdout = fs::read(&stdout_file).unwrap_or_default();
        let stderr = fs::read(&stderr_file).unwrap_or_default();
        
        // Clean up temporary files
        let _ = fs::remove_file(&wrapper_script);
        let _ = fs::remove_file(&stdout_file);
        let _ = fs::remove_file(&stderr_file);
        let _ = fs::remove_file(&exitcode_file);
        
        Ok(Output {
            status: ExitStatus::from_raw(actual_exit_code as u32),
            stdout,
            stderr,
        })
    }

    /// Execute with escalated privileges and stream output in real-time
    /// 
    /// Returns a channel receiver for CommandEvent messages and a CommandChild handle
    /// 
    /// # Examples
    ///
    /// ```no_run
    /// use elevated_command::Command;
    /// use std::process::Command as StdCommand;
    ///
    /// fn main() {
    ///     let mut cmd = StdCommand::new("path to the application");
    ///     let elevated_cmd = Command::new(cmd);
    ///     
    ///     let (rx, child) = elevated_cmd.spawn().unwrap();
    ///     
    ///     while let Ok(event) = rx.recv() {
    ///         match event {
    ///             CommandEvent::Stdout(data) => {
    ///                 println!("OUT: {}", String::from_utf8_lossy(&data));
    ///             }
    ///             CommandEvent::Stderr(data) => {
    ///                 eprintln!("ERR: {}", String::from_utf8_lossy(&data));
    ///             }
    ///             CommandEvent::Terminated { code } => {
    ///                 println!("Process exited with code: {:?}", code);
    ///                 break;
    ///             }
    ///             CommandEvent::Error(err) => {
    ///                 eprintln!("Error: {}", err);
    ///                 break;
    ///             }
    ///         }
    ///     }
    /// }
    /// ```
    pub fn spawn(self) -> Result<(Receiver<CommandEvent>, CommandChild)> {
        // Helper function to escape Windows command-line arguments
        fn windows_escape_arg(arg: &str) -> String {
            if arg.is_empty() {
                return "\"\"".to_string();
            }
            
            if !arg.chars().any(|c| c == ' ' || c == '\t' || c == '\n' || c == '\"' || c == '\\') {
                return arg.to_string();
            }
            
            let mut result = String::from("\"");
            let mut num_backslashes = 0;
            
            for c in arg.chars() {
                match c {
                    '\\' => {
                        num_backslashes += 1;
                    }
                    '"' => {
                        result.push_str(&"\\".repeat(num_backslashes * 2 + 1));
                        result.push('"');
                        num_backslashes = 0;
                    }
                    _ => {
                        if num_backslashes > 0 {
                            result.push_str(&"\\".repeat(num_backslashes));
                            num_backslashes = 0;
                        }
                        result.push(c);
                    }
                }
            }
            
            result.push_str(&"\\".repeat(num_backslashes * 2));
            result.push('"');
            result
        }

        // Create temporary files for output capture
        let temp_dir = env::temp_dir();
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis();
        let process_id = std::process::id();
        let unique_id = format!("{}_{}", process_id, timestamp);
        
        let stdout_file = temp_dir.join(format!("elevated_stdout_{}.txt", unique_id));
        let stderr_file = temp_dir.join(format!("elevated_stderr_{}.txt", unique_id));
        let exitcode_file = temp_dir.join(format!("elevated_exit_{}.txt", unique_id));
        
        // Build a wrapper batch script that captures output and exit code
        let wrapper_script = temp_dir.join(format!("elevated_wrapper_{}.bat", unique_id));
        
        let mut script_content = String::new();
        script_content.push_str("@echo off\r\n");
        script_content.push_str("setlocal\r\n");
        
        // Add environment variables
        for (k, v) in self.cmd.get_envs() {
            if let Some(value) = v {
                script_content.push_str(&format!("set \"{}={}\"\r\n",
                    k.to_str().unwrap(),
                    value.to_str().unwrap()
                ));
            }
        }
        
        // Build the command with escaped arguments
        let program = windows_escape_arg(self.cmd.get_program().to_str().unwrap());
        let args = self.cmd.get_args()
            .map(|c| windows_escape_arg(c.to_str().unwrap()))
            .collect::<Vec<String>>();
        
        // Execute command and redirect output to files
        script_content.push_str(&program);
        if !args.is_empty() {
            script_content.push_str(&format!(" {}", args.join(" ")));
        }
        script_content.push_str(&format!(" 1>\"{}\" 2>\"{}\"\r\n", 
            stdout_file.to_str().unwrap(),
            stderr_file.to_str().unwrap()
        ));
        
        // Save the actual exit code (not ShellExecuteW's return value)
        script_content.push_str(&format!("echo %ERRORLEVEL%>\"{}\"", exitcode_file.to_str().unwrap()));
        
        // Write the wrapper script
        fs::write(&wrapper_script, script_content.as_bytes())?;

        // Execute the wrapper script with elevation (non-blocking)
        let wrapper_script_str = wrapper_script.to_str().unwrap().to_string();
        thread::spawn(move || {
            unsafe { 
                ShellExecuteW(
                    HWND(0), 
                    w!("runas"), 
                    &HSTRING::from(&wrapper_script_str), 
                    &HSTRING::new(), 
                    PCWSTR::null(), 
                    SW_HIDE
                ) 
            };
        });

        // Create channel for events
        let (tx, rx) = channel();

        // Clone paths for the monitor thread
        let stdout_path = stdout_file.clone();
        let stderr_path = stderr_file.clone();
        let exitcode_path = exitcode_file.clone();
        let wrapper_path = wrapper_script.clone();

        // Spawn thread to monitor output files
        thread::spawn(move || {
            monitor_output_files_windows(tx, stdout_path, stderr_path, exitcode_path, wrapper_path);
        });

        Ok((
            rx,
            CommandChild {
                _output_dir: temp_dir,
            },
        ))
    }
}

// Monitor output files and send events through the channel (Windows version)
fn monitor_output_files_windows(
    tx: Sender<CommandEvent>,
    stdout_path: PathBuf,
    stderr_path: PathBuf,
    exitcode_path: PathBuf,
    wrapper_path: PathBuf,
) {
    let mut stdout_pos = 0u64;
    let mut stderr_pos = 0u64;
    let poll_interval = Duration::from_millis(100);
    let max_wait = Duration::from_secs(120); // Increased timeout for slow UAC
    let start = std::time::Instant::now();

    // Wait for UAC prompt and script to start
    // Don't wait too long - user might still be clicking UAC
    thread::sleep(Duration::from_millis(1000));

    let mut files_appeared = false;

    loop {
        // Check if we've timed out
        if start.elapsed() > max_wait {
            let _ = tx.send(CommandEvent::Error(
                "Timeout: Process did not start or complete. UAC may have been cancelled.".to_string()
            ));
            // Try to clean up
            let _ = fs::remove_file(&wrapper_path);
            let _ = fs::remove_file(&stdout_path);
            let _ = fs::remove_file(&stderr_path);
            let _ = fs::remove_file(&exitcode_path);
            break;
        }

        // Check if output files exist (means process started)
        if !files_appeared && (stdout_path.exists() || stderr_path.exists()) {
            files_appeared = true;
        }

        // Try to read new stdout data
        if let Ok(mut file) = File::open(&stdout_path) {
            if let Ok(metadata) = file.metadata() {
                let len = metadata.len();
                if len > stdout_pos {
                    let mut buffer = vec![0u8; (len - stdout_pos) as usize];
                    if file.seek(SeekFrom::Start(stdout_pos)).is_ok() {
                        if let Ok(n) = file.read(&mut buffer) {
                            buffer.truncate(n);
                            if !buffer.is_empty() {
                                let _ = tx.send(CommandEvent::Stdout(buffer));
                            }
                            stdout_pos += n as u64;
                        }
                    }
                }
            }
        }

        // Try to read new stderr data
        if let Ok(mut file) = File::open(&stderr_path) {
            if let Ok(metadata) = file.metadata() {
                let len = metadata.len();
                if len > stderr_pos {
                    let mut buffer = vec![0u8; (len - stderr_pos) as usize];
                    if file.seek(SeekFrom::Start(stderr_pos)).is_ok() {
                        if let Ok(n) = file.read(&mut buffer) {
                            buffer.truncate(n);
                            if !buffer.is_empty() {
                                let _ = tx.send(CommandEvent::Stderr(buffer));
                            }
                            stderr_pos += n as u64;
                        }
                    }
                }
            }
        }

        // Check if process has finished (exit code file exists)
        if exitcode_path.exists() {
            // Give it a moment to finish writing
            thread::sleep(Duration::from_millis(50));
            
            if let Ok(exitcode_data) = fs::read(&exitcode_path) {
                if let Ok(code_str) = String::from_utf8(exitcode_data) {
                    let code_str = code_str.trim();
                    if !code_str.is_empty() {
                        // Parse exit code
                        let exit_code = code_str.parse::<i32>().ok();
                        
                        // Read any remaining output
                        thread::sleep(Duration::from_millis(100));
                        
                        // Final stdout flush
                        if let Ok(mut file) = File::open(&stdout_path) {
                            if let Ok(metadata) = file.metadata() {
                                let len = metadata.len();
                                if len > stdout_pos {
                                    let mut buffer = vec![0u8; (len - stdout_pos) as usize];
                                    if file.seek(SeekFrom::Start(stdout_pos)).is_ok() {
                                        if let Ok(n) = file.read(&mut buffer) {
                                            buffer.truncate(n);
                                            if !buffer.is_empty() {
                                                let _ = tx.send(CommandEvent::Stdout(buffer));
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        // Final stderr flush
                        if let Ok(mut file) = File::open(&stderr_path) {
                            if let Ok(metadata) = file.metadata() {
                                let len = metadata.len();
                                if len > stderr_pos {
                                    let mut buffer = vec![0u8; (len - stderr_pos) as usize];
                                    if file.seek(SeekFrom::Start(stderr_pos)).is_ok() {
                                        if let Ok(n) = file.read(&mut buffer) {
                                            buffer.truncate(n);
                                            if !buffer.is_empty() {
                                                let _ = tx.send(CommandEvent::Stderr(buffer));
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        // Send termination event
                        let _ = tx.send(CommandEvent::Terminated { code: exit_code });
                        
                        // Clean up files
                        let _ = fs::remove_file(&stdout_path);
                        let _ = fs::remove_file(&stderr_path);
                        let _ = fs::remove_file(&exitcode_path);
                        let _ = fs::remove_file(&wrapper_path);
                        
                        break;
                    }
                }
            }
        }

        // Wait before next poll
        thread::sleep(poll_interval);
    }
}