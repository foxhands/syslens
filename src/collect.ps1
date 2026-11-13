$ErrorActionPreference = 'SilentlyContinue'
[Console]::OutputEncoding = [Text.Encoding]::UTF8
$r = [ordered]@{}
$o = Get-CimInstance Win32_OperatingSystem
$r.cs = Get-CimInstance Win32_ComputerSystem | Select-Object Name, Manufacturer, Model, TotalPhysicalMemory, SystemType, NumberOfProcessors, UserName, Domain, PCSystemType
$r.os = $o | Select-Object Caption, Version, BuildNumber, OSArchitecture, FreePhysicalMemory, TotalVisibleMemorySize, TotalVirtualMemorySize, FreeVirtualMemory, RegisteredUser, WindowsDirectory, SystemDirectory, OSLanguage,
    @{n='InstallDate'; e={ $_.InstallDate.ToString('dd.MM.yyyy HH:mm') }},
    @{n='LastBoot'; e={ $_.LastBootUpTime.ToString('dd.MM.yyyy HH:mm') }},
    @{n='UptimeSec'; e={ [int64]((Get-Date) - $_.LastBootUpTime).TotalSeconds }},
    @{n='Now'; e={ (Get-Date).ToString('dd.MM.yyyy / HH:mm') }}
$cv = Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion'
$r.osx = [ordered]@{ DisplayVersion = "$($cv.DisplayVersion)"; UBR = "$($cv.UBR)"; ProductName = "$($cv.ProductName)" }
$r.cpu = @(Get-CimInstance Win32_Processor | Select-Object Name, Manufacturer, NumberOfCores, NumberOfLogicalProcessors, MaxClockSpeed, CurrentClockSpeed, L2CacheSize, L3CacheSize, SocketDesignation, ProcessorId, Caption, AddressWidth, VirtualizationFirmwareEnabled, Description)
$r.cache = @(Get-CimInstance Win32_CacheMemory | Select-Object Purpose, Level, InstalledSize, Associativity)
$r.board = Get-CimInstance Win32_BaseBoard | Select-Object Manufacturer, Product, Version, SerialNumber
$r.bios = Get-CimInstance Win32_BIOS | Select-Object Manufacturer, SMBIOSBIOSVersion, SMBIOSMajorVersion, SMBIOSMinorVersion, @{n='Date'; e={ $_.ReleaseDate.ToString('dd.MM.yyyy') }}
$r.sysenc = Get-CimInstance Win32_ComputerSystemProduct | Select-Object Vendor, Name, Version, UUID
$r.mem = @(Get-CimInstance Win32_PhysicalMemory | Select-Object BankLabel, DeviceLocator, Capacity, Speed, ConfiguredClockSpeed, Manufacturer, PartNumber, SerialNumber, SMBIOSMemoryType, FormFactor, ConfiguredVoltage, DataWidth)
$r.slots = (Get-CimInstance Win32_PhysicalMemoryArray | Measure-Object MemoryDevices -Sum).Sum
$r.maxmem = (Get-CimInstance Win32_PhysicalMemoryArray | Measure-Object MaxCapacityEx -Sum).Sum
$r.gpu = @(Get-CimInstance Win32_VideoController | Select-Object Name, AdapterCompatibility, DriverVersion, VideoProcessor, AdapterRAM, AdapterDACType, CurrentHorizontalResolution, CurrentVerticalResolution, CurrentRefreshRate, CurrentBitsPerPixel, PNPDeviceID, @{n='DriverDate'; e={ $_.DriverDate.ToString('dd.MM.yyyy') }})
$vr = @()
Get-ChildItem 'HKLM:\SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}' | ForEach-Object {
    $p = Get-ItemProperty $_.PSPath
    $q = $p.'HardwareInformation.qwMemorySize'
    if ($q) { $vr += [pscustomobject]@{ Desc = "$($p.DriverDesc)"; Size = [uint64]$q } }
}
$r.vram = $vr
$basic = @{}
Get-CimInstance -Namespace root\wmi WmiMonitorBasicDisplayParams | ForEach-Object { $basic[$_.InstanceName] = $_ }
$r.mon = @(Get-CimInstance -Namespace root\wmi WmiMonitorID | ForEach-Object {
    $b = $basic[$_.InstanceName]
    [pscustomobject]@{
        WidthCm = $b.MaxHorizontalImageSize
        HeightCm = $b.MaxVerticalImageSize
        Name = (-join [char[]]($_.UserFriendlyName | Where-Object { $_ -ne 0 }))
        Maker = (-join [char[]]($_.ManufacturerName | Where-Object { $_ -ne 0 }))
        Serial = (-join [char[]]($_.SerialNumberID | Where-Object { $_ -ne 0 }))
        Year = $_.YearOfManufacture
    } })
$r.audio = @(Get-CimInstance Win32_SoundDevice | Select-Object Name, Manufacturer)
$r.disk = @(Get-CimInstance Win32_DiskDrive | Select-Object Model, Size, InterfaceType, MediaType, SerialNumber, FirmwareRevision, Index, Partitions, PNPDeviceID)
$r.pdisk = @(Get-PhysicalDisk | Select-Object FriendlyName, @{n='Media'; e={ "$($_.MediaType)" }}, @{n='Bus'; e={ "$($_.BusType)" }}, Size, DeviceId, @{n='Health'; e={ "$($_.HealthStatus)" }})
$r.vol = @(Get-CimInstance Win32_LogicalDisk | Where-Object { $_.DriveType -eq 3 -or $_.DriveType -eq 2 } | Select-Object DeviceID, VolumeName, FileSystem, Size, FreeSpace, DriveType)
$r.net = @(Get-CimInstance Win32_NetworkAdapter -Filter 'PhysicalAdapter=True' | Select-Object Name, MACAddress, Speed, NetEnabled, Manufacturer)
$r.ip = @(Get-CimInstance Win32_NetworkAdapterConfiguration -Filter 'IPEnabled=True' | Select-Object Description, IPAddress, DefaultIPGateway, DNSServerSearchOrder)
# --- SECOND HALF --- (programs and devices; the program runs it side by side with the first half)
$keys = 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\*',
        'HKLM:\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\*',
        'HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\*'
$seen = @{}
$r.apps = @(Get-ItemProperty $keys | Where-Object { $_.DisplayName -and -not $_.SystemComponent -and -not $_.ParentKeyName } |
    Sort-Object DisplayName | ForEach-Object {
        $k = "$($_.DisplayName)|$($_.DisplayVersion)"
        if (-not $seen[$k]) {
            $seen[$k] = 1
            [pscustomobject]@{ Name = "$($_.DisplayName)"; Version = "$($_.DisplayVersion)"; Publisher = "$($_.Publisher)"; Date = "$($_.InstallDate)"; SizeKb = $_.EstimatedSize }
        } })
# Programs from Microsoft Store and other packaged apps: those with a Start menu entry.
$start = @{}
Get-StartApps | ForEach-Object {
    $i = $_.AppID.IndexOf('!')
    if ($i -gt 0) { $f = $_.AppID.Substring(0, $i); if (-not $start[$f]) { $start[$f] = $_.Name } }
}
$store = @(Get-AppxPackage | Where-Object { -not $_.IsFramework -and $start[$_.PackageFamilyName] } | ForEach-Object {
    $n = $start[$_.PackageFamilyName]
    $k = "$n|$($_.Version)"
    if (-not $seen[$k]) {
        $seen[$k] = 1
        $pub = ''
        if ($_.Publisher -match 'O="?([^",]+)') { $pub = $Matches[1] }
        [pscustomobject]@{ Name = "$n"; Version = "$($_.Version)"; Publisher = $pub; Date = ''; SizeKb = 0; Store = $true }
    } })
$r.apps = @($r.apps) + $store
# Disk health counters are only given to an administrator.
$admin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
$r.admin = [bool]$admin
$r.rel = @()
if ($admin) {
    $r.rel = @(Get-PhysicalDisk | ForEach-Object {
        $c = $_ | Get-StorageReliabilityCounter
        if ($c) { [pscustomobject]@{ DeviceId = "$($_.DeviceId)"; Temperature = $c.Temperature; Wear = $c.Wear; PowerOnHours = $c.PowerOnHours; ReadErrors = $c.ReadErrorsTotal; WriteErrors = $c.WriteErrorsTotal } }
    })
}
$r.bat = @(Get-CimInstance Win32_Battery | Select-Object Name, EstimatedChargeRemaining, BatteryStatus, EstimatedRunTime)
$r.batx = [ordered]@{
    Full = (Get-CimInstance -Namespace root\wmi BatteryFullChargedCapacity | Measure-Object FullChargedCapacity -Sum).Sum
    Design = (Get-CimInstance -Namespace root\wmi BatteryStaticData | Measure-Object DesignedCapacity -Sum).Sum
    Cycles = (Get-CimInstance -Namespace root\wmi BatteryCycleCount | Measure-Object CycleCount -Maximum).Maximum
}
$r.startup = @(Get-CimInstance Win32_StartupCommand | Sort-Object Name | Select-Object Name, Command, @{n='Location'; e={ "$($_.Location)" -replace '^HKU\\S-[0-9-]+', 'HKCU' }})
$r.services = @(Get-CimInstance Win32_Service -Filter "StartMode='Auto'" | Sort-Object DisplayName | Select-Object DisplayName, Name, State)
$r.drivers = @(Get-CimInstance Win32_PnPSignedDriver | Where-Object { $_.DeviceName -and $_.DriverProviderName -and $_.DriverProviderName -notlike 'Microsoft*' } | Sort-Object DeviceName |
    Select-Object DeviceName, DriverVersion, DriverProviderName, @{n='Date'; e={ $_.DriverDate.ToString('dd.MM.yyyy') }})
$r.updates = @(Get-HotFix | Sort-Object InstalledOn -Descending | Select-Object HotFixID, Description, @{n='Date'; e={ $_.InstalledOn.ToString('dd.MM.yyyy') }})
$r.usb = @(Get-CimInstance Win32_PnPEntity | Where-Object { $_.PNPDeviceID -like 'USB\*' } | Sort-Object Name | Select-Object Name, Manufacturer, Status)
$r.printers = @(Get-CimInstance Win32_Printer | Sort-Object Name | Select-Object Name, DriverName, PortName, Default)
$r | ConvertTo-Json -Depth 5 -Compress
