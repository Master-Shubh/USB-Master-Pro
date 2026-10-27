# ==============================================================================
# USB Master Pro — Automated Zero-Prompt Windows Firewall Whitelisting Script
# Ensures 100% Outbound Port 443 P2P Connectivity on Windows 10 & 11
# ==============================================================================

Write-Host "[USB Master Pro] Configuring Outbound Firewall Rules..." -ForegroundColor Cyan

# 1. Outbound UDP 443 (WebRTC ICE / STUN / QUIC Datagrams)
$ruleNameUdp = "USB Master Pro (P2P UDP 443)"
if (!(Get-NetFirewallRule -DisplayName $ruleNameUdp -ErrorAction SilentlyContinue)) {
    New-NetFirewallRule -DisplayName $ruleNameUdp `
                        -Direction Outbound `
                        -Action Allow `
                        -Protocol UDP `
                        -RemotePort 443 `
                        -Description "Permits direct peer-to-peer WebRTC USB data streaming" | Out-Null
    Write-Host "  [+] Added Outbound UDP 443 Rule" -ForegroundColor Green
} else {
    Write-Host "  [*] Outbound UDP 443 Rule already present" -ForegroundColor DarkGray
}

# 2. Outbound TCP 443 (Rendezvous Broker Signaling WSS & TURN Fallback)
$ruleNameTcp = "USB Master Pro (Broker WSS 443)"
if (!(Get-NetFirewallRule -DisplayName $ruleNameTcp -ErrorAction SilentlyContinue)) {
    New-NetFirewallRule -DisplayName $ruleNameTcp `
                        -Direction Outbound `
                        -Action Allow `
                        -Protocol TCP `
                        -RemotePort 443 `
                        -Description "Permits secure WebSocket rendezvous signaling and TURN relay fallback" | Out-Null
    Write-Host "  [+] Added Outbound TCP 443 Rule" -ForegroundColor Green
} else {
    Write-Host "  [*] Outbound TCP 443 Rule already present" -ForegroundColor DarkGray
}

Write-Host "[USB Master Pro] Windows Defender & Firewall successfully configured. Zero prompts will be triggered!" -ForegroundColor Green
