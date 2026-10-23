# Hold a bound Windows socket without listening, reproducing a bind conflict
# which a connect-only port probe cannot see.
$ErrorActionPreference = 'Stop'
$socket = [System.Net.Sockets.Socket]::new(
    [System.Net.Sockets.AddressFamily]::InterNetwork,
    [System.Net.Sockets.SocketType]::Stream,
    [System.Net.Sockets.ProtocolType]::Tcp
)
try {
    $socket.ExclusiveAddressUse = $true
    $socket.Bind([System.Net.IPEndPoint]::new([System.Net.IPAddress]::Loopback, 8765))
    [Console]::Out.WriteLine('holding')
    [Console]::Out.Flush()
    $null = [Console]::ReadLine()
} finally {
    $socket.Dispose()
}
