#[cfg(test)]
mod pwsh {
    use crate::bindings::PowerShell;
    use crate::cli_xml::{parse_cli_xml, CliObject};
    use uuid::Uuid;

    #[test]
    fn load_pwsh_sdk_invoke_api() {
        let pwsh = PowerShell::new().unwrap();

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
        pwsh.invoke(true);

        let cmds_txt = pwsh.export_to_string("UtilityCommands");
        let pwsh_cmds: Vec<&str> = cmds_txt.lines().collect();

        println!("\nCommands (text):");
        for pwsh_cmd in &pwsh_cmds {
            println!("{}", &pwsh_cmd);
        }

        assert_eq!(pwsh_cmds.len(), 7);
        assert_eq!(pwsh_cmds.get(0), Some(&"Compare-Object"));
        assert_eq!(pwsh_cmds.get(1), Some(&"Group-Object"));
        assert_eq!(pwsh_cmds.get(2), Some(&"Measure-Object"));

        // Get-Date -UnixTimeSeconds 1577836800 | Set-Variable -Name Date
        pwsh.add_command("Get-Date");
        pwsh.add_parameter_long("-UnixTimeSeconds", 1577836800);
        pwsh.add_command("Set-Variable");
        pwsh.add_parameter_string("-Name", "Date");
        pwsh.add_statement();
        pwsh.invoke(true);

        let date_json = pwsh.export_to_json("Date");
        println!("\nDate (JSON):\n{}", &date_json);
        assert_eq!(&date_json, "\"2019-12-31T19:00:00-05:00\"");

        // Get-Verb -Verb Test | Set-Variable -Name Verb
        pwsh.add_script("Get-Verb -Verb Test");
        pwsh.add_command("Set-Variable");
        pwsh.add_parameter_string("-Name", "Verb");
        pwsh.add_statement();
        pwsh.invoke(true);

        let verb_xml = pwsh.export_to_xml("Verb");
        println!("\nVerb (XML):\n{}", &verb_xml);
        assert!(verb_xml.starts_with(
            "<Objs Version=\"1.1.0.1\" xmlns=\"http://schemas.microsoft.com/powershell/2004/04\">"
        ));
        assert!(verb_xml
            .find("<T>System.Management.Automation.VerbInfo</T>")
            .is_some());
        assert!(verb_xml
            .find("<ToString>System.Management.Automation.VerbInfo</ToString>")
            .is_some());
    }

    #[test]
    fn test_cli_xml() {
        // Get-VM IT-HELP-DVLS | Select-Object -Property VMId, VMName, State, Uptime, Status, Version
        let vm_xml = r#"<Objs Version="1.1.0.1" xmlns="http://schemas.microsoft.com/powershell/2004/04">
    <Obj RefId="0">
        <TN RefId="0">
            <T>Selected.Microsoft.HyperV.PowerShell.VirtualMachine</T>
            <T>System.Management.Automation.PSCustomObject</T>
            <T>System.Object</T>
        </TN>
        <MS>
            <G N="VMId">fbac8867-40ca-4032-a8e0-901c7f004cd7</G>
            <S N="VMName">IT-HELP-DVLS</S>
            <Obj N="State" RefId="1">
                <TN RefId="1">
                    <T>Microsoft.HyperV.PowerShell.VMState</T>
                    <T>System.Enum</T>
                    <T>System.ValueType</T>
                    <T>System.Object</T>
                </TN>
                <ToString>Off</ToString>
                <I32>3</I32>
            </Obj>
            <TS N="Uptime">PT0S</TS>
            <S N="Status">Operating normally</S>
            <S N="Version">10.0</S>
        </MS>
    </Obj>
</Objs>"#;

        let objs: Vec<CliObject> = parse_cli_xml(vm_xml);

        let vm_obj = objs.get(0).unwrap();

        let vmid_prop = vm_obj.values.get(0).unwrap();
        assert!(vmid_prop.is_guid());
        assert_eq!(vmid_prop.get_name(), Some("VMId"));
        assert_eq!(
            vmid_prop.as_guid(),
            Uuid::parse_str("fbac8867-40ca-4032-a8e0-901c7f004cd7")
                .ok()
                .as_ref()
        );

        let vmname_prop = vm_obj.values.get(1).unwrap();
        assert!(vmname_prop.is_string());
        assert_eq!(vmname_prop.get_name(), Some("VMName"));
        assert_eq!(vmname_prop.as_str(), Some("IT-HELP-DVLS"));

        let cmd_xml = r#"<Objs Version="1.1.0.1" xmlns="http://schemas.microsoft.com/powershell/2004/04">
  <Obj RefId="0">
    <TN RefId="0">
      <T>System.Diagnostics.Process</T>
      <T>System.ComponentModel.Component</T>
      <T>System.MarshalByRefObject</T>
      <T>System.Object</T>
    </TN>
    <ToString>System.Diagnostics.Process (cmd)</ToString>
    <Props>
      <S N="SafeHandle">Microsoft.Win32.SafeHandles.SafeProcessHandle</S>
      <S N="Handle">3084</S>
      <I32 N="BasePriority">8</I32>
      <B N="HasExited">false</B>
      <DT N="StartTime">2022-11-08T20:17:17.4042801-05:00</DT>
      <I32 N="Id">17804</I32>
      <S N="MachineName">.</S>
      <S N="MaxWorkingSet">1413120</S>
      <S N="MinWorkingSet">204800</S>
      <Obj N="Modules" RefId="1">
        <TN RefId="1">
          <T>System.Diagnostics.ProcessModuleCollection</T>
          <T>System.Collections.ReadOnlyCollectionBase</T>
          <T>System.Object</T>
        </TN>
        <IE>
          <S>System.Diagnostics.ProcessModule (cmd.exe)</S>
          <S>System.Diagnostics.ProcessModule (ntdll.dll)</S>
          <S>System.Diagnostics.ProcessModule (KERNEL32.dll)</S>
          <S>System.Diagnostics.ProcessModule (hmpalert.dll)</S>
          <S>System.Diagnostics.ProcessModule (KERNELBASE.dll)</S>
          <S>System.Diagnostics.ProcessModule (msvcrt.dll)</S>
          <S>System.Diagnostics.ProcessModule (combase.dll)</S>
          <S>System.Diagnostics.ProcessModule (ucrtbase.dll)</S>
          <S>System.Diagnostics.ProcessModule (RPCRT4.dll)</S>
          <S>System.Diagnostics.ProcessModule (winbrand.dll)</S>
          <S>System.Diagnostics.ProcessModule (shcore.dll)</S>
          <S>System.Diagnostics.ProcessModule (msvcp_win.dll)</S>
        </IE>
      </Obj>
      <I64 N="NonpagedSystemMemorySize64">6560</I64>
      <I32 N="NonpagedSystemMemorySize">6560</I32>
      <I64 N="PagedMemorySize64">5468160</I64>
      <I32 N="PagedMemorySize">5468160</I32>
      <I64 N="PagedSystemMemorySize64">49056</I64>
      <I32 N="PagedSystemMemorySize">49056</I32>
      <I64 N="PeakPagedMemorySize64">5468160</I64>
      <I32 N="PeakPagedMemorySize">5468160</I32>
      <I64 N="PeakWorkingSet64">5857280</I64>
      <I32 N="PeakWorkingSet">5857280</I32>
      <I64 N="PeakVirtualMemorySize64">2203383934976</I64>
      <I32 N="PeakVirtualMemorySize">65712128</I32>
      <B N="PriorityBoostEnabled">true</B>
      <S N="PriorityClass">Normal</S>
      <I64 N="PrivateMemorySize64">5468160</I64>
      <I32 N="PrivateMemorySize">5468160</I32>
      <S N="ProcessName">cmd</S>
      <S N="ProcessorAffinity">65535</S>
      <I32 N="SessionId">1</I32>
      <Obj N="Threads" RefId="2">
        <TN RefId="2">
          <T>System.Diagnostics.ProcessThreadCollection</T>
          <T>System.Collections.ReadOnlyCollectionBase</T>
          <T>System.Object</T>
        </TN>
        <IE>
          <S>System.Diagnostics.ProcessThread</S>
        </IE>
      </Obj>
      <I32 N="HandleCount">76</I32>
      <I64 N="VirtualMemorySize64">2203383930880</I64>
      <I32 N="VirtualMemorySize">65708032</I32>
      <B N="EnableRaisingEvents">false</B>
      <I64 N="WorkingSet64">5857280</I64>
      <I32 N="WorkingSet">5857280</I32>
      <Nil N="SynchronizingObject" />
      <S N="MainModule">System.Diagnostics.ProcessModule (cmd.exe)</S>
      <TS N="PrivilegedProcessorTime">PT0S</TS>
      <TS N="TotalProcessorTime">PT0.015625S</TS>
      <TS N="UserProcessorTime">PT0.015625S</TS>
      <S N="MainWindowHandle">264750</S>
      <S N="MainWindowTitle">Command Prompt</S>
      <B N="Responding">true</B>
      <Nil N="Site" />
      <Nil N="Container" />
    </Props>
    <MS>
      <S N="Name">cmd</S>
      <I32 N="SI">1</I32>
      <I32 N="Handles">76</I32>
      <I64 N="VM">2203383930880</I64>
      <I64 N="WS">5857280</I64>
      <I64 N="PM">5468160</I64>
      <I64 N="NPM">6560</I64>
      <S N="Path">C:\WINDOWS\system32\cmd.exe</S>
      <S N="CommandLine">"C:\WINDOWS\system32\cmd.exe" </S>
      <S N="Parent">System.Diagnostics.Process (explorer)</S>
      <S N="Company">Microsoft Corporation</S>
      <Db N="CPU">0.015625</Db>
      <S N="FileVersion">10.0.22000.1 (WinBuild.160101.0800)</S>
      <S N="ProductVersion">10.0.22000.1</S>
      <S N="Description">Windows Command Processor</S>
      <S N="Product">Microsoft® Windows® Operating System</S>
      <S N="__NounName">Process</S>
    </MS>
  </Obj>
</Objs>
"#;

        let _objs: Vec<CliObject> = parse_cli_xml(cmd_xml);
    }
}
