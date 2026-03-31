using System.Text.Json;
using System.Text.Json.Nodes;

if (args.Length < 3 || args.Length > 4)
{
    Console.Error.WriteLine(
        "Usage: BundlePwshLayout <publishDir> <hostRuntimeConfigPath> <pwshRuntimeConfigPath> [dotnetRoot]");
    return 1;
}

string publishDir = Path.GetFullPath(args[0]);
string hostRuntimeConfigPath = Path.GetFullPath(args[1]);
string pwshRuntimeConfigPath = Path.GetFullPath(args[2]);
string dotnetRoot = args.Length == 4
    ? Path.GetFullPath(args[3])
    : Path.GetDirectoryName(
        Environment.GetEnvironmentVariable("DOTNET_ROOT")
        ?? Environment.GetEnvironmentVariable("DOTNET_HOST_PATH")
        ?? throw new InvalidOperationException("Unable to resolve dotnet root. Pass it explicitly as the fourth argument."))!;

static (string Tfm, string NetCoreVersion, string? WindowsDesktopVersion) ReadHostFrameworkInfo(string runtimeConfigPath)
{
    JsonObject hostRuntimeConfig = JsonNode.Parse(File.ReadAllText(runtimeConfigPath))?.AsObject()
        ?? throw new InvalidOperationException("Host runtimeconfig.json was empty.");
    JsonObject runtimeOptions = hostRuntimeConfig["runtimeOptions"]?.AsObject()
        ?? throw new InvalidOperationException("Host runtimeconfig.json is missing runtimeOptions.");

    string tfm = runtimeOptions["tfm"]?.GetValue<string>() ?? "net10.0";
    string? netCoreVersion = null;
    string? windowsDesktopVersion = null;

    JsonArray? includedFrameworks = runtimeOptions["includedFrameworks"]?.AsArray();
    if (includedFrameworks is not null)
    {
        foreach (JsonNode? frameworkNode in includedFrameworks)
        {
            JsonObject framework = frameworkNode?.AsObject()
                ?? throw new InvalidOperationException("Framework entry was not an object.");
            string? name = framework["name"]?.GetValue<string>();
            string? version = framework["version"]?.GetValue<string>();

            if (string.Equals(name, "Microsoft.NETCore.App", StringComparison.Ordinal))
            {
                netCoreVersion = version;
            }
            else if (string.Equals(name, "Microsoft.WindowsDesktop.App", StringComparison.Ordinal))
            {
                windowsDesktopVersion = version;
            }
        }
    }

    netCoreVersion ??= runtimeOptions["frameworks"]?.AsArray()?
        .Select(node => node?.AsObject())
        .Where(node => string.Equals(node?["name"]?.GetValue<string>(), "Microsoft.NETCore.App", StringComparison.Ordinal))
        .Select(node => node?["version"]?.GetValue<string>())
        .FirstOrDefault();

    if (string.IsNullOrWhiteSpace(netCoreVersion))
    {
        throw new InvalidOperationException("Unable to resolve Microsoft.NETCore.App version from host runtimeconfig.");
    }

    return (tfm, netCoreVersion, windowsDesktopVersion);
}

static void CopyDirectory(string sourceDir, string destinationDir)
{
    if (Directory.Exists(destinationDir))
    {
        Directory.Delete(destinationDir, recursive: true);
    }

    Directory.CreateDirectory(destinationDir);

    foreach (string directory in Directory.GetDirectories(sourceDir, "*", SearchOption.AllDirectories))
    {
        string relativePath = Path.GetRelativePath(sourceDir, directory);
        Directory.CreateDirectory(Path.Combine(destinationDir, relativePath));
    }

    foreach (string file in Directory.GetFiles(sourceDir, "*", SearchOption.AllDirectories))
    {
        string relativePath = Path.GetRelativePath(sourceDir, file);
        string destinationPath = Path.Combine(destinationDir, relativePath);
        Directory.CreateDirectory(Path.GetDirectoryName(destinationPath)!);
        File.Copy(file, destinationPath, overwrite: true);
    }
}

static void UpdatePwshRuntimeConfig(
    string pwshRuntimeConfigPath,
    string tfm,
    string netCoreVersion,
    string? windowsDesktopVersion)
{
    JsonObject pwshRuntimeConfig = JsonNode.Parse(File.ReadAllText(pwshRuntimeConfigPath))?.AsObject()
        ?? throw new InvalidOperationException("pwsh.runtimeconfig.json was empty.");
    JsonObject runtimeOptions = pwshRuntimeConfig["runtimeOptions"]?.AsObject()
        ?? throw new InvalidOperationException("pwsh.runtimeconfig.json is missing runtimeOptions.");

    runtimeOptions["tfm"] = tfm.Replace("-windows", string.Empty, StringComparison.OrdinalIgnoreCase);
    runtimeOptions.Remove("includedFrameworks");

    JsonArray frameworks =
    [
        new JsonObject
        {
            ["name"] = "Microsoft.NETCore.App",
            ["version"] = netCoreVersion
        }
    ];

    if (!string.IsNullOrWhiteSpace(windowsDesktopVersion))
    {
        frameworks.Add(new JsonObject
        {
            ["name"] = "Microsoft.WindowsDesktop.App",
            ["version"] = windowsDesktopVersion
        });
    }

    runtimeOptions["frameworks"] = frameworks;

    File.WriteAllText(
        pwshRuntimeConfigPath,
        pwshRuntimeConfig.ToJsonString(new JsonSerializerOptions { WriteIndented = true }));
}

static void CopySharedFramework(
    string dotnetRoot,
    string publishDir,
    string frameworkName,
    string frameworkVersion)
{
    string sourceDir = Path.Combine(dotnetRoot, "shared", frameworkName, frameworkVersion);
    if (!Directory.Exists(sourceDir))
    {
        throw new DirectoryNotFoundException($"Missing shared framework directory: {sourceDir}");
    }

    string destinationDir = Path.Combine(publishDir, "shared", frameworkName, frameworkVersion);
    CopyDirectory(sourceDir, destinationDir);
}

(string tfm, string netCoreVersion, string? windowsDesktopVersion) = ReadHostFrameworkInfo(hostRuntimeConfigPath);
UpdatePwshRuntimeConfig(pwshRuntimeConfigPath, tfm, netCoreVersion, windowsDesktopVersion);
CopySharedFramework(dotnetRoot, publishDir, "Microsoft.NETCore.App", netCoreVersion);

if (!string.IsNullOrWhiteSpace(windowsDesktopVersion))
{
    CopySharedFramework(dotnetRoot, publishDir, "Microsoft.WindowsDesktop.App", windowsDesktopVersion);
}

return 0;
