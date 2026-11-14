# Devolutions.MultiPwsh.Cli

`Devolutions.MultiPwsh.Cli` ships RID-specific `multi-pwsh` native binaries for .NET projects. It also includes opt-in AppHost MSBuild targets for projects that need to copy the same binary as a PowerShell replacement apphost.

The normal CLI payload is copied under `runtimes/<rid>/native/` for build and publish outputs. Package authors can disable that content copy and consume neutral apphost metadata instead:

```xml
<ItemGroup>
  <PackageReference Include="Devolutions.MultiPwsh.Cli" Version="0.19.0" PrivateAssets="all" />
</ItemGroup>

<PropertyGroup>
  <MultiPwshRuntimeNativeContentCopyEnabled>false</MultiPwshRuntimeNativeContentCopyEnabled>
</PropertyGroup>

<Target Name="StagePowerShellAppHosts" DependsOnTargets="ResolveMultiPwshAppHostAssets">
  <ItemGroup>
    <PowerShellSdkAppHost Include="@(MultiPwshAppHostAsset)">
      <PackagePath>tools/apphost/%(RuntimeIdentifier)/%(AppHostFileName)</PackagePath>
    </PowerShellSdkAppHost>
  </ItemGroup>
</Target>
```

`@(MultiPwshAppHostAsset)` includes the source path as the item identity plus `RuntimeIdentifier`, `NativeFileName`, `AppHostFileName`, `PackageRelativePath`, and `PackageId` metadata. The package also provides `MultiPwshAppHostSupportedRuntimeIdentifiers` and `MultiPwshAppHostManifestPath`.

AppHost mode is inert by default; set `MultiPwshAppHostEnabled=true` to copy the selected RID binary as `multi-pwsh`, `pwsh`, or another explicit file name.

```xml
<ItemGroup>
  <PackageReference Include="Devolutions.MultiPwsh.Cli" Version="0.19.0" PrivateAssets="all" />
</ItemGroup>

<PropertyGroup>
  <MultiPwshAppHostEnabled>true</MultiPwshAppHostEnabled>
  <MultiPwshAppHostOutputBaseName>pwsh</MultiPwshAppHostOutputBaseName>
</PropertyGroup>
```

Downstream SDK packages can copy the binary as `pwsh` or `pwsh.exe` beside their own `pwsh.dll` and `pwsh.runtimeconfig.json`, or place it under `runtimes/<rid>/native/` with the shared PowerShell payload at the publish root. In both layouts, `multi-pwsh` runs the local payload directly instead of resolving `pwsh` from PATH.
