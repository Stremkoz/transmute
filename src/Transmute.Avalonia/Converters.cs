using System.Globalization;
using Avalonia.Data.Converters;

namespace Transmute.Avalonia;

public static class Converters
{
    /// <summary>Count == 0 → true. Drives the empty-state visibility.</summary>
    public static readonly IValueConverter IsZero =
        new FuncValueConverter<int, bool>(count => count == 0);

    /// <summary>Count > 0 → true. Drives the populated-queue visibility.</summary>
    public static readonly IValueConverter IsNonZero =
        new FuncValueConverter<int, bool>(count => count > 0);

    /// <summary>Two-way boolean inversion for paired Lossless/Lossy and Skip/Only toggles.</summary>
    public static readonly IValueConverter InverseBool = new InverseBoolConverter();
}

public sealed class InverseBoolConverter : IValueConverter
{
    public object? Convert(object? value, Type targetType, object? parameter, CultureInfo culture) =>
        value is bool b ? !b : value;

    public object? ConvertBack(object? value, Type targetType, object? parameter, CultureInfo culture) =>
        value is bool b ? !b : value;
}
