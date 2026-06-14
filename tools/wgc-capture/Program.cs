// Windows Graphics Capture (WGC) window screenshot tool
// Same API used by Teams, Zoom, and Discord for window capture.
// Captures GPU-rendered content from any virtual desktop without bringing
// the window to the foreground.
//
// Usage: wgc-capture.exe <hwnd> <output.png>
//   hwnd: window handle as decimal integer

using System.Runtime.InteropServices;
using Windows.Graphics;
using Windows.Graphics.Capture;
using Windows.Graphics.DirectX;
using Windows.Graphics.DirectX.Direct3D11;
using Windows.Graphics.Imaging;
using Windows.Storage.Streams;

if (args.Length < 2)
{
    Console.Error.WriteLine("Usage: wgc-capture <hwnd> <output.png>");
    return 1;
}

if (!long.TryParse(args[0], out long hwndLong))
{
    Console.Error.WriteLine("Invalid hwnd");
    return 1;
}

var outputPath = args[1];
var hwnd = new IntPtr(hwndLong);

if (!GraphicsCaptureSession.IsSupported())
{
    Console.Error.WriteLine("Windows Graphics Capture not supported on this system (requires Windows 10 1903+)");
    return 1;
}

try
{
    // Initialize WinRT (MTA) — required before RoGetActivationFactory
    int roHr = RoInitialize(1); // RO_INIT_MULTITHREADED
    if (roHr < 0 && roHr != unchecked((int)0x80010106)) // RPC_E_CHANGED_MODE is OK
        Marshal.ThrowExceptionForHR(roHr);

    // ── 1. Create D3D11 device ──────────────────────────────────────────────
    int hr = D3D11CreateDevice(
        IntPtr.Zero, 1 /*D3D_DRIVER_TYPE_HARDWARE*/, IntPtr.Zero,
        0x20 /*D3D11_CREATE_DEVICE_BGRA_SUPPORT*/,
        IntPtr.Zero, 0, 7 /*D3D11_SDK_VERSION*/,
        out IntPtr d3dDevicePtr, out _, out IntPtr d3dContextPtr);
    if (hr < 0) Marshal.ThrowExceptionForHR(hr);

    // ── 2. QI for IDXGIDevice, wrap as WinRT IDirect3DDevice ───────────────
    var dxgiIid = new Guid("54ec77fa-1377-44e6-8c32-88fd5f44c84c"); // IDXGIDevice
    Marshal.ThrowExceptionForHR(Marshal.QueryInterface(d3dDevicePtr, ref dxgiIid, out IntPtr dxgiDevicePtr));

    hr = CreateDirect3D11DeviceFromDXGIDevice(dxgiDevicePtr, out IntPtr winrtDevicePtr);
    if (hr < 0) Marshal.ThrowExceptionForHR(hr);

    var d3dDevice = (IDirect3DDevice)Marshal.GetObjectForIUnknown(winrtDevicePtr);

    Marshal.Release(dxgiDevicePtr);
    Marshal.Release(d3dDevicePtr);
    Marshal.Release(d3dContextPtr);

    // ── 3. Create GraphicsCaptureItem for the window ────────────────────────
    // RoGetActivationFactory requires an HSTRING, not a plain string
    var className = "Windows.Graphics.Capture.GraphicsCaptureItem";
    WindowsCreateString(className, className.Length, out IntPtr hstrClass);
    var factoryIid = new Guid("00000035-0000-0000-C000-000000000046"); // IActivationFactory
    int factHr = RoGetActivationFactory(hstrClass, ref factoryIid, out IntPtr factoryPtr);
    WindowsDeleteString(hstrClass);
    Console.Error.WriteLine($"[diag] RoGetActivationFactory hr=0x{factHr:X8} ptr={factoryPtr}");
    if (factHr < 0) Marshal.ThrowExceptionForHR(factHr);
    var interop  = (IGraphicsCaptureItemInterop)Marshal.GetObjectForIUnknown(factoryPtr);
    Marshal.Release(factoryPtr);
    var itemIid  = new Guid("79C3F95B-31F7-4EC2-A464-632EF5D30760"); // IGraphicsCaptureItem
    Console.Error.WriteLine($"[diag] calling CreateForWindow hwnd={hwnd}");
    var itemPtr  = interop.CreateForWindow(hwnd, ref itemIid);
    Console.Error.WriteLine($"[diag] CreateForWindow returned ptr={itemPtr}");
    var item     = GraphicsCaptureItem.FromAbi(itemPtr);
    Console.Error.WriteLine($"[diag] FromAbi item={item?.DisplayName}");
    Marshal.Release(itemPtr);

    // ── 4. Capture one frame ─────────────────────────────────────────────────
    var size = item.Size; // SizeInt32 in physical pixels

    using var framePool = Direct3D11CaptureFramePool.Create(
        d3dDevice,
        DirectXPixelFormat.B8G8R8A8UIntNormalized,
        1,   // buffer count
        size);

    using var session = framePool.CreateCaptureSession(item);
    session.IsCursorCaptureEnabled = false;

    Direct3D11CaptureFrame? capturedFrame = null;
    var ready = new SemaphoreSlim(0, 1);

    framePool.FrameArrived += (pool, _) =>
    {
        capturedFrame = pool.TryGetNextFrame();
        ready.Release();
    };

    session.StartCapture();

    if (!await ready.WaitAsync(TimeSpan.FromSeconds(10)))
        throw new TimeoutException("No frame received within 10 seconds");

    session.Dispose();

    using var frame = capturedFrame!;

    // ── 5. Convert GPU surface → SoftwareBitmap → PNG ───────────────────────
    var softBitmap = await SoftwareBitmap.CreateCopyFromSurfaceAsync(
        frame.Surface, BitmapAlphaMode.Premultiplied);

    // Convert to BGRA8 premultiplied (required by BitmapEncoder for PNG)
    if (softBitmap.BitmapPixelFormat != BitmapPixelFormat.Bgra8 ||
        softBitmap.BitmapAlphaMode   != BitmapAlphaMode.Premultiplied)
    {
        var converted = SoftwareBitmap.Convert(softBitmap,
            BitmapPixelFormat.Bgra8, BitmapAlphaMode.Premultiplied);
        softBitmap.Dispose();
        softBitmap = converted;
    }

    using var mem = new InMemoryRandomAccessStream();
    var encoder = await BitmapEncoder.CreateAsync(BitmapEncoder.PngEncoderId, mem);
    encoder.SetSoftwareBitmap(softBitmap);
    encoder.BitmapTransform.InterpolationMode = BitmapInterpolationMode.NearestNeighbor;
    await encoder.FlushAsync();

    var reader = new DataReader(mem.GetInputStreamAt(0));
    await reader.LoadAsync((uint)mem.Size);
    var bytes = new byte[mem.Size];
    reader.ReadBytes(bytes);
    await File.WriteAllBytesAsync(outputPath, bytes);

    Console.WriteLine($"Saved {bytes.Length} bytes → {outputPath}");
    return 0;
}
catch (Exception ex)
{
    Console.Error.WriteLine($"Error: {ex.Message}");
    return 1;
}

// ── P/Invoke & COM declarations ──────────────────────────────────────────────

[DllImport("d3d11.dll", CallingConvention = CallingConvention.Winapi)]
static extern int D3D11CreateDevice(
    IntPtr pAdapter, int driverType, IntPtr software, uint flags,
    IntPtr pFeatureLevels, uint featureLevelCount, uint sdkVersion,
    out IntPtr ppDevice, out int featureLevel, out IntPtr ppImmediateContext);

[DllImport("d3d11.dll", CallingConvention = CallingConvention.Winapi,
    EntryPoint = "CreateDirect3D11DeviceFromDXGIDevice")]
static extern int CreateDirect3D11DeviceFromDXGIDevice(IntPtr dxgiDevice, out IntPtr graphicsDevice);

[DllImport("combase.dll", CallingConvention = CallingConvention.Winapi, CharSet = CharSet.Unicode)]
static extern int RoGetActivationFactory(string activatableClassId, ref Guid iid, out IntPtr factory);

[DllImport("combase.dll", CallingConvention = CallingConvention.Winapi)]
static extern int RoInitialize(uint initType);

[ComImport, Guid("3628E81B-3CAC-4C60-B7F4-23CE0E0C3356")]
[InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
interface IGraphicsCaptureItemInterop
{
    IntPtr CreateForWindow(IntPtr window, ref Guid iid);
    IntPtr CreateForMonitor(IntPtr monitor, ref Guid iid);
}
