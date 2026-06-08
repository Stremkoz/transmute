using System.Windows;
using System.Windows.Controls;
using Transmute.GUI.ViewModels;

namespace Transmute.GUI;

public class QueueEntryTemplateSelector : DataTemplateSelector
{
    public DataTemplate? FileTemplate { get; set; }
    public DataTemplate? FolderTemplate { get; set; }

    public override DataTemplate? SelectTemplate(object item, DependencyObject container) =>
        item is FolderEntryViewModel ? FolderTemplate : FileTemplate;
}
