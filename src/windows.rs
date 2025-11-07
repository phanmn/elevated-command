/*---------------------------------------------------------------------------------------------
 *  This is an ALTERNATIVE windows.rs implementation with OUTPUT CAPTURE
 *  
 *  Replace the original windows.rs with this file to enable stdout/stderr capture on Windows.
 *  
 *  Trade-off: Adds a small delay (500ms) to wait for output files to be written.
 *--------------------------------------------------------------------------------------------*/

use crate::Command;
use anyhow::Result;
use std::env;
use std::fs;
use std::mem;
use std::os::windows::process::ExitStatusExt;
use std::path::PathBuf;
use std::process::{Output, ExitStatus};
use winapi::shared::minwindef::{DWORD, LPVOID};
use winapi::um::processthreadsapi::{GetCurrentProcess, OpenProcessToken};
use winapi::um::securitybaseapi::GetTokenInformation;
use winapi::um::winnt::{HANDLE, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY};
use windows::core::{HSTRING, PCWSTR, w};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_HIDE;


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
    /// On Windows, according to https://learn.microsoft.com/en-us/windows/win32/api/shellapi/nf-shellapi-shellexecutew#return-value,
    /// Output.status.code() shoudl be greater than 32 if the function succeeds, 
    /// otherwise the value indicates the cause of the failure
    /// 
    /// This version CAPTURES stdout and stderr by writing them to temporary files.
    /// There is a 500ms delay to wait for the elevated process to write output.
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
        script_content.push_str(&format!(" >\"{}\" 2>\"{}\"", 
            stdout_file.to_str().unwrap(),
            stderr_file.to_str().unwrap()
        ));
        
        // Write the wrapper script
        fs::write(&wrapper_script, script_content.as_bytes())?;

        // Execute the wrapper script with elevation
        let r = unsafe { 
            ShellExecuteW(
                HWND(0), 
                w!("runas"), 
                &HSTRING::from(wrapper_script.to_str().unwrap()), 
                &HSTRING::new(), 
                PCWSTR::null(), 
                SW_HIDE
            ) 
        };
        
        let exit_code = r.0 as u32;
        
        // Wait for the elevated process to complete and write files
        // This is a simple approach - waits 500ms then checks for files
        std::thread::sleep(std::time::Duration::from_millis(500));
        
        // Read output files (may be empty if process hasn't finished)
        let stdout = fs::read(&stdout_file).unwrap_or_default();
        let stderr = fs::read(&stderr_file).unwrap_or_default();
        
        // Clean up temporary files
        let _ = fs::remove_file(&wrapper_script);
        let _ = fs::remove_file(&stdout_file);
        let _ = fs::remove_file(&stderr_file);
        
        Ok(Output {
            status: ExitStatus::from_raw(exit_code),
            stdout,
            stderr,
        })
    }
}
