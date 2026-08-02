# Hosted-spawn regression fixture: mirrors Windows PowerShell -File rejecting a lone "-".
# See https://github.com/PowerShell/PowerShell/issues/10510
foreach ($a in $args) {
    if ($a -eq '-') {
        $err = New-Object System.Management.Automation.PSArgumentException (
            'Cannot process argument because the value of argument "name" is not valid. Change the value of the "name" argument and run the operation again.'
        )
        $host.UI.WriteErrorLine(($MyInvocation.MyCommand.Path) + ' : ' + $err.Message)
        exit 1
    }
}
Write-Output '{"type":"system","subtype":"init","session_id":"chat-dash-trap"}'
Write-Output '{"type":"result","subtype":"success","session_id":"chat-dash-trap","result":"ok"}'
exit 0
