@echo off
:: Disconnect RDP session while keeping the Windows GUI session unlocked.
:: This pushes the active RDP session to the local console, so the physical
:: monitor shows the lock screen but the session keeps rendering in the
:: background — allowing bots to take screenshots, click, etc.
::
:: Requires Administrator privileges (tscon.exe moves sessions).
:: Intended to be run via a Scheduled Task with /rl highest.

FOR /F "skip=1 tokens=3" %%s IN ('query user %USERNAME%') DO (
    %windir%\System32\tscon.exe %%s /dest:console
)
