#[cfg(test)]
mod pwsh {
    use crate::bindings::{PowerShell};

    #[test]
    fn load_pwsh_sdk_invoke_api() {
        let pwsh = PowerShell::new().unwrap();
		
		let mut output_file = std::env::temp_dir();
        output_file.push("pwsh-cmds.txt");
		let output_file_str = output_file.to_str().unwrap();
		println!("Output File:\n{}", &output_file_str);

		// Get-Command -CommandType Cmdlet -Name *-Object -Module Microsoft.PowerShell.Utility |
		// Select-Object -ExpandProperty Name | Set-Variable -Name UtilityCommands
		pwsh.add_command("Get-Command");
		pwsh.add_parameter_string("-CommandType", "Cmdlet");
		pwsh.add_parameter_string("-Name", "*-Object");
		pwsh.add_parameter_string("-Module", "Microsoft.PowerShell.Utility");
		pwsh.add_command("Select-Object");
		pwsh.add_parameter_string("-ExpandProperty", "Name");
		pwsh.add_command("Set-Variable");
		pwsh.add_parameter_string("-Name", "UtilityCommands");
		pwsh.add_statement();
		pwsh.invoke();

		// Remove-Item -Path '/tmp/pwsh-cmds.txt' -ErrorAction SilentlyContinue
		pwsh.add_script(format!("Remove-Item -Path '{}' -ErrorAction SilentlyContinue", &output_file_str).as_str());
		pwsh.invoke();

		// $UtilityCommands | Set-Content -Path '/tmp/pwsh-cmds.txt' -Force
		pwsh.add_script(format!("$UtilityCommands | Set-Content -Path '{}' -Force", &output_file_str).as_str());
		pwsh.invoke();

		let output_data = std::fs::read_to_string(output_file.as_path()).unwrap();

		let pwsh_cmds: Vec<&str> = output_data.lines().collect();

		for pwsh_cmd in &pwsh_cmds {
			println!("{}", &pwsh_cmd);
		}

		assert_eq!(pwsh_cmds.len(), 7);
		assert_eq!(pwsh_cmds.get(0), Some(&"Compare-Object"));
		assert_eq!(pwsh_cmds.get(1), Some(&"Group-Object"));
		assert_eq!(pwsh_cmds.get(2), Some(&"Measure-Object"));

    }
}
