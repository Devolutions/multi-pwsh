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

    /*
    $MyObj = [PSCustomObject]@{
        MyString = "Purée"
        MyChar = [char] 'à'
        MyBool = [bool] $true
        MyDateTime = [System.DateTime]::new(633435181522731993)
        MyDuration = [System.TimeSpan]::new(90269026)
        MyU8 = [byte] 254
        MyI8 = [sbyte] -127
        MyU16 = [ushort] 65535
        MyI16 = [short] -32767
        MyU32 = [uint] 4294967295
        MyI32 = [int] -2147483648
        MyU64 = [ulong] 18446744073709551615
        MyI64 = [long] -9223372036854775808
        MyFloat = [float] 12.34
        MyDouble = [double] 34.56
        MyDecimal = [decimal] 56.78
        MyBuffer = [System.Convert]::FromBase64String('AQIDBA==')
        MyGuid = [System.Guid]::new('792e5b37-4505-47ef-b7d2-8711bb7affa8')
        MyUri = [System.Uri]::new('http://www.microsoft.com/')
        MyVersion = [System.Version]::new('6.2.1.3')
        MyXmlDocument = [xml] '<item><name>laptop</name></item>'
        MyScriptBlock = [scriptblock] {Get-Command -Type Cmdlet}
        MyNull = $null
    }
    [System.Management.Automation.PSSerializer]::Serialize($MyObj)
    */

    #[test]
    fn test_cli_xml_primitive() {
        let obj_xml = r#"
<Objs Version="1.1.0.1" xmlns="http://schemas.microsoft.com/powershell/2004/04">
    <Obj RefId="0">
        <TN RefId="0">
            <T>System.Management.Automation.PSCustomObject</T>
            <T>System.Object</T>
        </TN>
        <MS>
            <S N="MyString">Purée</S>
            <C N="MyChar">224</C>
            <B N="MyBool">true</B>
            <DT N="MyDateTime">2008-04-11T13:42:32.2731993</DT>
            <TS N="MyDuration">PT9.0269026S</TS>
            <By N="MyU8">254</By>
            <SB N="MyI8">-127</SB>
            <U16 N="MyU16">65535</U16>
            <I16 N="MyI16">-32767</I16>
            <U32 N="MyU32">4294967295</U32>
            <I32 N="MyI32">-2147483648</I32>
            <U64 N="MyU64">18446744073709551615</U64>
            <I64 N="MyI64">-9223372036854775808</I64>
            <Sg N="MyFloat">12.34</Sg>
            <Db N="MyDouble">34.56</Db>
            <D N="MyDecimal">56.78</D>
            <BA N="MyBuffer">AQIDBA==</BA>
            <G N="MyGuid">792e5b37-4505-47ef-b7d2-8711bb7affa8</G>
            <URI N="MyUri">http://www.microsoft.com/</URI>
            <Version N="MyVersion">6.2.1.3</Version>
            <XD N="MyXmlDocument">&lt;item&gt;&lt;name&gt;laptop&lt;/name&gt;&lt;/item&gt;</XD>
            <SBK N="MyScriptBlock">Get-Command -Type Cmdlet</SBK>
            <Nil N="MyNull" />
        </MS>
    </Obj>
</Objs>"#;

        let objs: Vec<CliObject> = parse_cli_xml(obj_xml);

        let obj = objs.get(0).unwrap();

        let string_prop = obj.values.get(0).unwrap();
        assert!(string_prop.is_string());
        assert_eq!(string_prop.as_str(), Some("Purée"));

        let char_prop = obj.values.get(1).unwrap();
        assert!(char_prop.is_char());
        assert_eq!(char_prop.as_char(), Some('à'));

        let bool_prop = obj.values.get(2).unwrap();
        assert!(bool_prop.is_bool());
        assert_eq!(bool_prop.as_bool(), Some(true));

        let datetime_prop = obj.values.get(3).unwrap();
        assert!(datetime_prop.is_datetime());

        let duration_prop = obj.values.get(4).unwrap();
        assert!(duration_prop.is_duration());

        let uint8_prop = obj.values.get(5).unwrap();
        assert!(uint8_prop.is_uint8());
        assert_eq!(uint8_prop.as_u8(), Some(254));

        let int8_prop = obj.values.get(6).unwrap();
        assert!(int8_prop.is_int8());
        assert_eq!(int8_prop.as_i8(), Some(-127));

        let uint16_prop = obj.values.get(7).unwrap();
        assert!(uint16_prop.is_uint16());
        assert_eq!(uint16_prop.as_u16(), Some(65535));

        let int16_prop = obj.values.get(8).unwrap();
        assert!(int16_prop.is_int16());
        assert_eq!(int16_prop.as_i16(), Some(-32767));

        let uint32_prop = obj.values.get(9).unwrap();
        assert!(uint32_prop.is_uint32());
        assert_eq!(uint32_prop.as_u32(), Some(4294967295));

        let int32_prop = obj.values.get(10).unwrap();
        assert!(int32_prop.is_int32());
        assert_eq!(int32_prop.as_i32(), Some(-2147483648));

        let uint64_prop = obj.values.get(11).unwrap();
        assert!(uint64_prop.is_uint64());
        assert_eq!(uint64_prop.as_u64(), Some(18446744073709551615));

        let int64_prop = obj.values.get(12).unwrap();
        assert!(int64_prop.is_int64());
        assert_eq!(int64_prop.as_i64(), Some(-9223372036854775808));

        let float_prop = obj.values.get(13).unwrap();
        assert!(float_prop.is_float());
        assert_eq!(float_prop.as_float(), Some(12.34));

        let double_prop = obj.values.get(14).unwrap();
        assert!(double_prop.is_double());
        assert_eq!(double_prop.as_double(), Some(34.56));

        let decimal_prop = obj.values.get(15).unwrap();
        assert!(decimal_prop.is_decimal());

        let buffer_prop = obj.values.get(16).unwrap();
        assert!(buffer_prop.is_buffer());
        assert_eq!(buffer_prop.as_bytes(), Some(vec![1, 2, 3, 4u8].as_ref()));

        let guid_prop = obj.values.get(17).unwrap();
        assert!(guid_prop.is_guid());
        assert_eq!(
            guid_prop.as_guid(),
            uuid::Uuid::parse_str("792e5b37-4505-47ef-b7d2-8711bb7affa8")
                .ok()
                .as_ref()
        );

        let uri_prop = obj.values.get(18).unwrap();
        assert!(uri_prop.is_uri());
        assert_eq!(
            uri_prop.as_uri(),
            url::Url::parse("http://www.microsoft.com/").ok().as_ref()
        );

        let version_prop = obj.values.get(19).unwrap();
        assert!(version_prop.is_version());
        assert_eq!(version_prop.as_version(), Some("6.2.1.3"));

        let xml_document_prop = obj.values.get(20).unwrap();
        assert!(xml_document_prop.is_xml_document());
        assert_eq!(
            xml_document_prop.as_xml_document(),
            Some("&lt;item&gt;&lt;name&gt;laptop&lt;/name&gt;&lt;/item&gt;")
        );

        let script_block_prop = obj.values.get(21).unwrap();
        assert!(script_block_prop.is_script_block());
        assert_eq!(
            script_block_prop.as_script_block(),
            Some("Get-Command -Type Cmdlet")
        );

        let null_prop = obj.values.get(22).unwrap();
        assert!(null_prop.is_null());
    }

    #[test]
    fn test_cli_xml_complex() {
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
