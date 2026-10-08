namespace Devolutions.PowerShell.Ffi;

public sealed class PowerShellInvocationError
{
    internal PowerShellInvocationError(
        string message,
        string fullyQualifiedErrorId,
        string category,
        string exceptionType,
        int exceptionHResult,
        string invocationName,
        string positionMessage,
        string scriptStackTrace,
        string categoryReason,
        string categoryActivity,
        string categoryTargetName,
        string categoryTargetType,
        string commandName,
        string invocationLine,
        string offsetInLine,
        string pipelineLength,
        string pipelinePosition,
        string errorDetailsMessage,
        string recommendedAction,
        string targetDisplayText,
        PowerShellValue? targetValue,
        ulong sequence,
        bool isTruncated)
    {
        Message = message;
        FullyQualifiedErrorId = fullyQualifiedErrorId;
        Category = category;
        ExceptionType = exceptionType;
        ExceptionHResult = exceptionHResult;
        InvocationName = invocationName;
        PositionMessage = positionMessage;
        ScriptStackTrace = scriptStackTrace;
        CategoryReason = categoryReason;
        CategoryActivity = categoryActivity;
        CategoryTargetName = categoryTargetName;
        CategoryTargetType = categoryTargetType;
        CommandName = commandName;
        InvocationLine = invocationLine;
        OffsetInLine = offsetInLine;
        PipelineLength = pipelineLength;
        PipelinePosition = pipelinePosition;
        ErrorDetailsMessage = errorDetailsMessage;
        RecommendedAction = recommendedAction;
        TargetDisplayText = targetDisplayText;
        TargetValue = targetValue;
        Sequence = sequence;
        IsTruncated = isTruncated;
    }

    public string Message { get; }

    public string FullyQualifiedErrorId { get; }

    public string Category { get; }

    public string ExceptionType { get; }

    public int ExceptionHResult { get; }

    public string InvocationName { get; }

    public string PositionMessage { get; }

    public string ScriptStackTrace { get; }

    public string CategoryReason { get; }

    public string CategoryActivity { get; }

    public string CategoryTargetName { get; }

    public string CategoryTargetType { get; }

    public string CommandName { get; }

    public string InvocationLine { get; }

    public string OffsetInLine { get; }

    public string PipelineLength { get; }

    public string PipelinePosition { get; }

    public string ErrorDetailsMessage { get; }

    public string RecommendedAction { get; }

    public string TargetDisplayText { get; }

    public PowerShellValue? TargetValue { get; }

    public ulong Sequence { get; }

    public bool IsTruncated { get; }
}
