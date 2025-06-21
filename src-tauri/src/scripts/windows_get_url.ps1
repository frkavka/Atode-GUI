# Windows Browser Info Retrieval Script
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$OutputEncoding = [System.Text.Encoding]::UTF8

Add-Type -TypeDefinition @"
    using System;
    using System.Runtime.InteropServices;
    using System.Text;
    
    public class Win32API {
        [DllImport("user32.dll", CharSet = CharSet.Unicode)]
        public static extern IntPtr GetForegroundWindow();
        
        [DllImport("user32.dll", CharSet = CharSet.Unicode)]
        public static extern int GetWindowText(IntPtr hWnd, StringBuilder text, int count);
        
        [DllImport("user32.dll")]
        public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint processId);
        
        [DllImport("user32.dll")]
        public static extern bool SetForegroundWindow(IntPtr hWnd);
        
        [DllImport("user32.dll")]
        public static extern void keybd_event(byte bVk, byte bScan, int dwFlags, int dwExtraInfo);
        
        public const int KEYEVENTF_KEYUP = 0x0002;
        public const byte VK_CONTROL = 0x11;
        public const byte VK_L = 0x4C;
        public const byte VK_C = 0x43;
        public const byte VK_ESCAPE = 0x1B;
    }
"@

Add-Type -AssemblyName System.Windows.Forms

function Get-URLFromAddressBar {
    param($ProcessName)
    
    try {
        Write-Host "Getting URL from address bar: $ProcessName" -ForegroundColor Yellow
        
        $originalClipboard = ""
        try {
            $originalClipboard = [System.Windows.Forms.Clipboard]::GetText()
        } catch {
            # Ignore
        }
        $url = $null
                
        # Method 1: Ctrl+L -> C (fast)
        [Win32API]::keybd_event([Win32API]::VK_CONTROL, 0, 0, 0)
        [Win32API]::keybd_event([Win32API]::VK_L, 0, 0, 0)
        Start-Sleep -Milliseconds 30
        [Win32API]::keybd_event([Win32API]::VK_C, 0, 0, 0)
        [Win32API]::keybd_event([Win32API]::VK_L, 0, [Win32API]::KEYEVENTF_KEYUP, 0)
        [Win32API]::keybd_event([Win32API]::VK_C, 0, [Win32API]::KEYEVENTF_KEYUP, 0)
        [Win32API]::keybd_event([Win32API]::VK_CONTROL, 0, [Win32API]::KEYEVENTF_KEYUP, 0)
        Start-Sleep -Milliseconds 50
        
        try {
            $url = [System.Windows.Forms.Clipboard]::GetText().Trim()
            if ($url -and (($url -match '^https?://') -or ($url -match '^file://'))) {
                Write-Host "Fast method success: $url" -ForegroundColor Green
                # Escape key
                [Win32API]::keybd_event([Win32API]::VK_ESCAPE, 0, 0, 0)
                Start-Sleep -Milliseconds 50
                [Win32API]::keybd_event([Win32API]::VK_ESCAPE, 0, [Win32API]::KEYEVENTF_KEYUP, 0)
                # Restore clipboard
                try {
                    if ($originalClipboard) {
                        [System.Windows.Forms.Clipboard]::SetText($originalClipboard)
                    }
                } catch { }
                return $url
            }
        } catch {
            Write-Host "Fast method failed" -ForegroundColor Yellow
        }

        
        # Method 2: F6 twice
        [Win32API]::keybd_event(0x75, 0, 0, 0)
        [Win32API]::keybd_event(0x75, 0, [Win32API]::KEYEVENTF_KEYUP, 0)
        Start-Sleep -Milliseconds 20
        [Win32API]::keybd_event(0x75, 0, 0, 0)
        [Win32API]::keybd_event(0x75, 0, [Win32API]::KEYEVENTF_KEYUP, 0)
        Start-Sleep -Milliseconds 50
        
        # Select all and copy
        [Win32API]::keybd_event([Win32API]::VK_CONTROL, 0, 0, 0)
        [Win32API]::keybd_event(0x41, 0, 0, 0)
        Start-Sleep -Milliseconds 50
        [Win32API]::keybd_event(0x41, 0, [Win32API]::KEYEVENTF_KEYUP, 0)
        [Win32API]::keybd_event([Win32API]::VK_CONTROL, 0, [Win32API]::KEYEVENTF_KEYUP, 0)
        Start-Sleep -Milliseconds 50
        
        # Copy
        [Win32API]::keybd_event([Win32API]::VK_CONTROL, 0, 0, 0)
        [Win32API]::keybd_event([Win32API]::VK_C, 0, 0, 0)
        Start-Sleep -Milliseconds 50
        [Win32API]::keybd_event([Win32API]::VK_C, 0, [Win32API]::KEYEVENTF_KEYUP, 0)
        [Win32API]::keybd_event([Win32API]::VK_CONTROL, 0, [Win32API]::KEYEVENTF_KEYUP, 0)
        Start-Sleep -Milliseconds 50

        try {
            $url = [System.Windows.Forms.Clipboard]::GetText().Trim()
            if ($url -and (($url -match '^https?://') -or ($url -match '^file://'))) {
                Write-Host "Reliable method success: $url" -ForegroundColor Green
            } else {
                Write-Host "All methods failed" -ForegroundColor Red
                $url = $null
            }
        } catch {
            Write-Host "Reliable method failed" -ForegroundColor Red
            $url = $null
        }
        
        # Escape key
        [Win32API]::keybd_event([Win32API]::VK_ESCAPE, 0, 0, 0)
        Start-Sleep -Milliseconds 50
        [Win32API]::keybd_event([Win32API]::VK_ESCAPE, 0, [Win32API]::KEYEVENTF_KEYUP, 0)
        
        # Restore clipboard
        try {
            if ($originalClipboard) {
                [System.Windows.Forms.Clipboard]::SetText($originalClipboard)
            }
        } catch { }
        
        return $url
        
    } catch {
        Write-Host "URL retrieval error: $($_.Exception.Message)" -ForegroundColor Red
        return $null
    }
}

function Get-BrowserInfoKeyboard {
    try {
        $hwnd = [Win32API]::GetForegroundWindow()
        if ($hwnd -eq [IntPtr]::Zero) {
            throw "WINDOW_NOT_FOUND"
        }
        
        $title = New-Object System.Text.StringBuilder 2048
        $titleLength = [Win32API]::GetWindowText($hwnd, $title, $title.Capacity)
        
        if ($titleLength -eq 0) {
            throw "TITLE_NOT_FOUND"
        }
        
        $windowTitle = $title.ToString()
        
        $processId = 0
        [Win32API]::GetWindowThreadProcessId($hwnd, [ref]$processId) | Out-Null
        
        $process = Get-Process -Id $processId -ErrorAction SilentlyContinue
        if (-not $process) {
            throw "PROCESS_NOT_FOUND"
        }
        
        $processName = $process.ProcessName
        
        $browserProcesses = @("chrome", "firefox", "edge", "brave", `
                              "opera", "vivaldi", "msedge", "iexplore", "safari")
        if ($processName.ToLower() -notin $browserProcesses) {
            Write-Output "NOT_BROWSER|$processName|not_browser"
            return
        }
        
        Write-Host "Browser detected: $processName" -ForegroundColor Green
        
        $cleanTitle = $windowTitle
        switch ($processName.ToLower()) {
            "chrome" { $cleanTitle = $cleanTitle -replace " - Google Chrome.*$", "" }
            "firefox" { $cleanTitle = $cleanTitle -replace " — Mozilla Firefox.*$", "" `
                        -replace " - Mozilla Firefox.*$", "" }
            "msedge" { $cleanTitle = $cleanTitle -replace " - Microsoft Edge.*$", "" }
            "edge" { $cleanTitle = $cleanTitle -replace " - Microsoft Edge.*$", "" }
            "brave" { $cleanTitle = $cleanTitle -replace " - Brave.*$", "" }
            "opera" { $cleanTitle = $cleanTitle -replace " - Opera.*$", "" }
            "vivaldi" { $cleanTitle = $cleanTitle -replace " - Vivaldi.*$", "" }
        }
        
        [Win32API]::SetForegroundWindow($hwnd) | Out-Null
        Start-Sleep -Milliseconds 100
        
        $actualUrl = Get-URLFromAddressBar -ProcessName $processName
        
        if ($actualUrl -and (    ($actualUrl -match '^https?://')`
                             -or ($actualUrl -match '^file://'))) {
            $finalUrl = $actualUrl
        } else {
            Write-Host "Fallback to title guessing" -ForegroundColor Yellow
            if ($windowTitle -match "Claude") {
                $finalUrl = "https://claude.ai/chat"
            } elseif ($windowTitle -match "GitHub") {
                $finalUrl = "https://github.com"
            } elseif ($windowTitle -match "Stack Overflow") {
                $finalUrl = "https://stackoverflow.com"
            } elseif ($windowTitle -match "YouTube") {
                $finalUrl = "https://www.youtube.com"
            } elseif ($windowTitle -match "Google") {
                $finalUrl = "https://www.google.com"
            } else {
                $finalUrl = "https://example.com/failed"
            }
        }
        
        if ($finalUrl -match '^file://') {
            $hostName = $env:COMPUTERNAME
            $cleanTitle = "[LocalFile_$hostName] $cleanTitle"
        }
        
        Write-Output "$finalUrl|$cleanTitle|$processName"
        
    } catch {
        Write-Output "ERROR|$($_.Exception.Message)|unknown"
    }
}

Get-BrowserInfoKeyboard