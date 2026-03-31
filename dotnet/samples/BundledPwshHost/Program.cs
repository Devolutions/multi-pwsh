using System.Management.Automation;

namespace BundledPwshHost;

internal static class Program
{
    private static int Main()
    {
        using PowerShell powerShell = PowerShell.Create();
        powerShell.AddScript("$PSVersionTable | Select-Object PSVersion, PSEdition | ConvertTo-Json -Compress");

        var results = powerShell.Invoke();
        if (powerShell.HadErrors)
        {
            foreach (ErrorRecord error in powerShell.Streams.Error)
            {
                Console.Error.WriteLine(error);
            }

            return 1;
        }

        Console.WriteLine("Managed PowerShell SDK invocation:");
        foreach (PSObject result in results)
        {
            Console.WriteLine(result.BaseObject);
        }

        Console.WriteLine($"Base directory: {AppContext.BaseDirectory}");
        Console.WriteLine("Publish this project with a RID to assemble a bundled pwsh payload.");
        return 0;
    }
}
