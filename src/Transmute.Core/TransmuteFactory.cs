using Transmute.Core.Backends;
using Transmute.Core.Config;
using Transmute.Core.Discovery;
using Transmute.Core.Processing;
using Transmute.Core.Routing;

namespace Transmute.Core;

public class TransmuteFactory
{
    public static (ConversionEngine Engine, FormatRouter Router, TempFileManager Temp, IReadOnlyList<IBackend> Backends)
        Create(AppConfig config)
    {
        var discovery = new BinaryDiscovery(config.Binaries);

        var jxl = new JxlBackend(discovery.Resolve("cjxl"), discovery.Resolve("djxl"));
        var webp = new WebPBackend(discovery.Resolve("cwebp"), discovery.Resolve("dwebp"));
        var ffmpeg = new FfmpegBackend(discovery.Resolve("ffmpeg"));
        var vips = new VipsBackend();
        var magick = new MagickBackend(discovery.Resolve("magick"));

        var temp = new TempFileManager(config.Processing.TempDirectory);
        var router = new FormatRouter(jxl, webp, ffmpeg, vips, magick, temp);
        var engine = new ConversionEngine(router, config.Processing.MaxParallelJobs);

        IReadOnlyList<IBackend> backends = [jxl, webp, ffmpeg, vips, magick];

        return (engine, router, temp, backends);
    }
}
