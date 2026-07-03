use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};

use crate::app::{App, MonitorSort};
use crate::event::Mode;
use crate::procs::{self, ProcessRow};

const MONITOR_MEM_WIDTH: usize = 8;
const MONITOR_CPU_WIDTH: usize = 7;
const MONITOR_COL_GAP: usize = 2;
const MONITOR_COMMAND_WIDTH: usize = 28;

fn monitor_pane_width(inner_width: usize) -> usize {
    let fixed = MONITOR_MEM_WIDTH
        + MONITOR_COL_GAP
        + MONITOR_CPU_WIDTH
        + MONITOR_COL_GAP
        + MONITOR_COMMAND_WIDTH
        + MONITOR_COL_GAP;
    inner_width.saturating_sub(fixed).max(1)
}

fn monitor_detail_items(row: &ProcessRow) -> Vec<ListItem<'_>> {
    let dim = Style::default().add_modifier(Modifier::DIM);
    let mut lines: Vec<String> = Vec::new();
    lines.push(format!("  ├ {}", row.command));
    lines.push(format!("  ├ cwd: {}", row.pane.cwd));
    lines.push(format!("  ├ pid: {}", row.pid));
    if row.ancestors.is_empty() {
        lines.push("  └ (none)".to_string());
    } else {
        let ancestor_count = row.ancestors.len();
        for i in 0..ancestor_count {
            let ancestor = &row.ancestors[i];
            let connector = if i + 1 == ancestor_count {
                "  └ "
            } else {
                "  ├ "
            };
            lines.push(format!(
                "{}{} ({})",
                connector, ancestor.command, ancestor.pid
            ));
        }
    }
    lines
        .into_iter()
        .map(|text| ListItem::new(Line::from(Span::styled(text, dim))))
        .collect()
}

fn format_monitor_cell(value: &str, width: usize, align_right: bool) -> String {
    let truncated = procs::truncate_chars(value, width);
    if align_right {
        format!("{:>width$}", truncated, width = width)
    } else {
        format!("{:<width$}", truncated, width = width)
    }
}

pub fn render_monitor(frame: &mut Frame, app: &mut App, area: Rect) {
    let sort_label = match app.monitor_sort {
        MonitorSort::Mem => "MEM",
        MonitorSort::Cpu => "CPU",
    };
    let title = format!(" Process Monitor (sort: {}) ", sort_label);

    let outer_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(area);

    let block = Block::default().borders(Borders::ALL).title(title);
    let inner = block.inner(outer_chunks[0]);
    frame.render_widget(block, outer_chunks[0]);

    let inner_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Min(0)])
        .split(inner);

    let pane_width = monitor_pane_width(inner.width as usize);
    let gap = " ".repeat(MONITOR_COL_GAP);

    let header = Line::from(vec![
        Span::styled(
            format_monitor_cell("MEM", MONITOR_MEM_WIDTH, true),
            Style::default().fg(Color::DarkGray),
        ),
        Span::raw(gap.clone()),
        Span::styled(
            format_monitor_cell("CPU", MONITOR_CPU_WIDTH, true),
            Style::default().fg(Color::DarkGray),
        ),
        Span::raw(gap.clone()),
        Span::styled(
            format_monitor_cell("COMMAND", MONITOR_COMMAND_WIDTH, false),
            Style::default().fg(Color::DarkGray),
        ),
        Span::raw(gap),
        Span::styled(
            format_monitor_cell("PANE", pane_width, false),
            Style::default().fg(Color::DarkGray),
        ),
    ]);
    frame.render_widget(Paragraph::new(header), inner_chunks[0]);

    let show_detail = app.mode == Mode::ProcessDetail;
    let mut items: Vec<ListItem> = Vec::new();
    for i in 0..app.monitor_rows.len() {
        let row = &app.monitor_rows[i];
        let mem = format_monitor_cell(&procs::format_rss_kb(row.rss_kb), MONITOR_MEM_WIDTH, true);
        let cpu = format_monitor_cell(&procs::format_pcpu(row.pcpu), MONITOR_CPU_WIDTH, true);
        let command = format_monitor_cell(
            &procs::command_basename(&row.command),
            MONITOR_COMMAND_WIDTH,
            false,
        );
        let pane = format_monitor_cell(&procs::format_pane_label(&row.pane), pane_width, false);
        let line = Line::from(vec![
            Span::raw(mem),
            Span::raw("  "),
            Span::raw(cpu),
            Span::raw("  "),
            Span::raw(command),
            Span::raw("  "),
            Span::raw(pane),
        ]);
        items.push(ListItem::new(line));
        if show_detail && i == app.monitor_selected {
            items.extend(monitor_detail_items(row));
        }
    }

    let list = List::new(items).highlight_style(app.highlight_style);

    frame.render_stateful_widget(list, inner_chunks[1], &mut app.monitor_list_state);

    let footer = Paragraph::new(
        "[j/k] move  [s] sort  [space] details  [enter] switch  [x] kill  [esc/q] back",
    )
    .style(Style::default().fg(Color::DarkGray));
    frame.render_widget(footer, outer_chunks[1]);
}
