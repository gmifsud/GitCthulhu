using Avalonia;
using Avalonia.Automation;
using Avalonia.Controls;
using Avalonia.Headless;
using Avalonia.Headless.XUnit;
using Avalonia.Input;
using Avalonia.VisualTree;
using Resolve.App.Shell;
using Resolve.App.Tests.Support;
using Resolve.Core.Resolution;

namespace Resolve.App.Tests;

public class ConflictNavigatorTests
{
    private static string Name(Control item) => AutomationProperties.GetName(item) ?? string.Empty;

    [AvaloniaFact]
    public void Items_name_the_region_its_lines_and_its_state()
    {
        var nav = new ConflictNavigator(Sessions.Criterion1());
        Assert.Equal("Conflict 1 of 1, lines 20–20, unresolved", Name(nav.Items()[0]));
        Assert.Equal("Change 1 of 2, lines 3–3, taken from Current branch", Name(nav.Items()[1]));
        Assert.Equal("Change 2 of 2, lines 40–40, taken from Incoming branch", Name(nav.Items()[2]));
        Assert.Equal(3, nav.Items().Count);
    }

    [AvaloniaFact]
    public void The_status_line_is_a_polite_live_region() =>
        Assert.Equal(AutomationLiveSetting.Polite, AutomationProperties.GetLiveSetting(new ConflictNavigator(Sessions.Criterion1()).Status));

    [AvaloniaFact]
    public void Announce_sets_the_status_text()
    {
        var nav = new ConflictNavigator(Sessions.Criterion1());
        nav.Announce("Conflict 1 resolved");
        Avalonia.Threading.Dispatcher.UIThread.RunJobs();
        Assert.Equal("Conflict 1 resolved", nav.Status.Text);
    }

    [AvaloniaFact]
    public void Announcing_the_same_text_again_clears_the_line_first_so_it_is_re_read()
    {
        var nav = new ConflictNavigator(Sessions.Criterion1());
        var seen = new List<string?>();
        nav.Status.PropertyChanged += (_, e) =>
        {
            if (e.Property == TextBlock.TextProperty)
            {
                seen.Add(nav.Status.Text);
            }
        };
        nav.Announce("Undone");
        Avalonia.Threading.Dispatcher.UIThread.RunJobs();
        seen.Clear();

        nav.Announce("Undone");
        Assert.True(string.IsNullOrEmpty(nav.Status.Text), "the line must be cleared synchronously");
        Avalonia.Threading.Dispatcher.UIThread.RunJobs();

        Assert.Equal("Undone", nav.Status.Text);
        Assert.Equal(2, seen.Count);
        Assert.True(string.IsNullOrEmpty(seen[0]));
        Assert.Equal("Undone", seen[1]);
    }

    [AvaloniaFact]
    public void Only_the_left_mouse_button_chooses_a_region()
    {
        var session = Sessions.Criterion1();
        var nav = new ConflictNavigator(session);
        var window = new Window { Width = 600, Height = 400, Content = nav };
        window.Show();
        window.UpdateLayout();
        var chosen = new List<int>();
        nav.RegionChosen += chosen.Add;
        var item = nav.Items()[1];
        var point = item.TranslatePoint(new Avalonia.Point(5, 5), window)!.Value;

        window.MouseDown(point, MouseButton.Right);
        window.MouseUp(point, MouseButton.Right);
        Assert.Empty(chosen);

        window.MouseDown(point, MouseButton.Left);
        window.MouseUp(point, MouseButton.Left);
        Assert.Single(chosen);
    }

    [AvaloniaFact]
    public void Names_follow_the_document_after_a_resolution()
    {
        var session = Sessions.Criterion1();
        var nav = new ConflictNavigator(session);
        var conflict = session.Document.Regions.Single(r => r.Kind == Resolve.Core.Merge.RegionKind.Conflict);
        nav.Select(conflict.RegionId);

        session.Document.Apply(new RegionCommand.Accept(conflict.RegionId, ResolutionChoice.RightThenLeft));
        nav.Refresh();
        Assert.Equal("Conflict 1 of 1, lines 20–21, both, Incoming branch first", Name(nav.Items()[0]));
        Assert.Equal(conflict.RegionId, nav.CurrentRegionId);

        session.Document.Apply(new RegionCommand.Accept(conflict.RegionId, ResolutionChoice.Left));
        nav.Refresh();
        Assert.Equal("Conflict 1 of 1, lines 20–20, taken from Current branch", Name(nav.Items()[0]));

        session.Document.Apply(new RegionCommand.Accept(conflict.RegionId, ResolutionChoice.Base));
        nav.Refresh();
        Assert.Equal("Conflict 1 of 1, lines 20–20, kept the base", Name(nav.Items()[0]));
    }

    [AvaloniaFact]
    public void Enter_on_an_item_chooses_that_region()
    {
        var session = Sessions.Criterion1();
        var nav = new ConflictNavigator(session);
        var window = new Window { Width = 600, Height = 400, Content = nav };
        window.Show();
        window.UpdateLayout();
        var chosen = new List<int>();
        nav.RegionChosen += chosen.Add;

        var change = session.Document.Regions.Where(r => r.Kind != Resolve.Core.Merge.RegionKind.Equal && r.Kind != Resolve.Core.Merge.RegionKind.Conflict).First();
        nav.Select(change.RegionId);
        nav.Items()[1].Focus();
        window.KeyPressQwerty(PhysicalKey.Enter, RawInputModifiers.None);

        Assert.Equal([change.RegionId], chosen);
        Assert.Equal(change.RegionId, nav.CurrentRegionId);
    }

    [AvaloniaFact]
    public void Select_does_not_raise_RegionChosen()
    {
        var session = Sessions.Criterion1();
        var nav = new ConflictNavigator(session);
        var raised = false;
        nav.RegionChosen += _ => raised = true;
        nav.Select(session.Document.Regions.First(r => r.Kind != Resolve.Core.Merge.RegionKind.Equal).RegionId);
        Assert.False(raised);
    }

    [AvaloniaFact]
    public void Items_are_focusable()
    {
        var nav = new ConflictNavigator(Sessions.Criterion1());
        Assert.All(nav.Items(), item => Assert.True(item.Focusable));
    }
}
