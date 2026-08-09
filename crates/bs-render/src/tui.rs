use crate::TreeNode;
use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Terminal,
};
use std::io;

struct FlatItem {
    depth: usize,
    name: String,
    file: String,
    weight: f32,
    confidence: f32,
    mark: Option<String>,
    has_children: bool,
    expanded: bool,
    children: Vec<TreeNode>,
}

pub fn run_tui(nodes: &[TreeNode], title: &str, weight_desc: &str) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    stdout.execute(EnterAlternateScreen)?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run_loop(&mut terminal, nodes, title, weight_desc);

    disable_raw_mode()?;
    io::stdout().execute(LeaveAlternateScreen)?;

    result
}

fn run_loop<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    nodes: &[TreeNode],
    title: &str,
    weight_desc: &str,
) -> Result<()> {
    let mut items = flatten(nodes, 0, true);
    let mut list_state = ListState::default();
    if !items.is_empty() {
        list_state.select(Some(0));
    }
    let mut filter = String::new();
    let mut filter_mode = false;

    loop {
        terminal.draw(|f| {
            let area = f.area();

            let chunk_sizes = if filter_mode {
                vec![
                    Constraint::Min(3),
                    Constraint::Length(3),
                    Constraint::Length(2),
                    Constraint::Length(1),
                ]
            } else {
                vec![
                    Constraint::Min(3),
                    Constraint::Length(2),
                    Constraint::Length(1),
                ]
            };

            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints(chunk_sizes)
                .split(area);

            let filter_lower = filter.to_lowercase();
            let indices: Vec<usize> = items
                .iter()
                .enumerate()
                .filter(|(_, item)| {
                    filter.is_empty()
                        || item.name.to_lowercase().contains(&filter_lower)
                        || item.file.to_lowercase().contains(&filter_lower)
                })
                .map(|(i, _)| i)
                .collect();

            let list_items: Vec<ListItem> = indices
                .iter()
                .map(|&idx| {
                    let item = &items[idx];
                    let indent = "  ".repeat(item.depth);
                    let prefix = if item.has_children {
                        if item.expanded {
                            "▼ "
                        } else {
                            "▶ "
                        }
                    } else {
                        "  "
                    };
                    let bar = weight_bar(item.weight);
                    let conf_tag = if item.confidence < 0.99 {
                        format!(" ┄{:.1}", item.confidence)
                    } else {
                        String::new()
                    };
                    let mark_str = item.mark.as_deref().unwrap_or("");

                    let color = match item.mark.as_deref() {
                        Some("+") => Color::Green,
                        Some("-") => Color::Red,
                        Some("~") => Color::Yellow,
                        _ => {
                            if item.weight > 0.7 {
                                Color::White
                            } else {
                                Color::Gray
                            }
                        }
                    };

                    let text = format!(
                        "{}{}{}{}{} {} {:.2}",
                        indent, prefix, mark_str, item.name, conf_tag, bar, item.weight
                    );
                    ListItem::new(Line::from(vec![Span::styled(
                        text,
                        Style::default().fg(color),
                    )]))
                })
                .collect();

            let list_title = format!(" borescope — {} ", title);
            let list = List::new(list_items)
                .block(Block::default().borders(Borders::ALL).title(list_title))
                .highlight_style(
                    Style::default()
                        .bg(Color::DarkGray)
                        .add_modifier(Modifier::BOLD),
                );

            f.render_stateful_widget(list, chunks[0], &mut list_state);

            // Detail panel: show selected node info + weight description
            let (detail_chunk, help_chunk) = if filter_mode {
                (chunks[2], chunks[3])
            } else {
                (chunks[1], chunks[2])
            };

            let detail_text = if let Some(idx) = list_state.selected() {
                if idx < items.len() {
                    let item = &items[idx];
                    let conf_str = if item.confidence < 0.99 {
                        format!("  confidence: {:.2}", item.confidence)
                    } else {
                        String::new()
                    };
                    format!(
                        " {}  score: {:.2}{}  file: {}",
                        weight_desc, item.weight, conf_str, item.file
                    )
                } else {
                    format!(" {}", weight_desc)
                }
            } else {
                format!(" {}", weight_desc)
            };

            let detail = Paragraph::new(detail_text).style(Style::default().fg(Color::Cyan));
            f.render_widget(detail, detail_chunk);

            if filter_mode {
                let filter_paragraph = Paragraph::new(format!("/{}", filter))
                    .block(Block::default().borders(Borders::ALL).title(" filter "));
                f.render_widget(filter_paragraph, chunks[1]);
                let help = Paragraph::new(" esc: cancel filter");
                f.render_widget(help, help_chunk);
            } else {
                let help =
                    Paragraph::new(" j/k: navigate  enter/space: expand  /: filter  q: quit");
                f.render_widget(help, help_chunk);
            }
        })?;

        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if filter_mode {
                    match key.code {
                        KeyCode::Esc => {
                            filter_mode = false;
                        }
                        KeyCode::Backspace => {
                            filter.pop();
                        }
                        KeyCode::Char(c) => {
                            filter.push(c);
                        }
                        _ => {}
                    }
                } else {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => break,
                        KeyCode::Char('j') | KeyCode::Down => {
                            let i = list_state.selected().unwrap_or(0);
                            if i + 1 < items.len() {
                                list_state.select(Some(i + 1));
                            }
                        }
                        KeyCode::Char('k') | KeyCode::Up => {
                            let i = list_state.selected().unwrap_or(0);
                            if i > 0 {
                                list_state.select(Some(i - 1));
                            }
                        }
                        KeyCode::Char('g') => {
                            list_state.select(Some(0));
                        }
                        KeyCode::Char('G') if !items.is_empty() => {
                            list_state.select(Some(items.len() - 1));
                        }
                        KeyCode::Enter | KeyCode::Char(' ') => {
                            if let Some(idx) = list_state.selected() {
                                toggle(&mut items, idx);
                            }
                        }
                        KeyCode::Char('/') => {
                            filter_mode = true;
                            filter.clear();
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    Ok(())
}

fn flatten(nodes: &[TreeNode], depth: usize, expand_top: bool) -> Vec<FlatItem> {
    let mut result = Vec::new();
    for node in nodes {
        let has_children = !node.children.is_empty();
        let expanded = expand_top && has_children;
        let children = node.children.clone();
        result.push(FlatItem {
            depth,
            name: node.name.clone(),
            file: node.file.clone(),
            weight: node.weight,
            confidence: node.confidence,
            mark: node.mark.clone(),
            has_children,
            expanded,
            children: children.clone(),
        });
        if expanded {
            result.extend(flatten(&children, depth + 1, false));
        }
    }
    result
}

fn toggle(items: &mut Vec<FlatItem>, idx: usize) {
    if !items[idx].has_children {
        return;
    }
    let was_expanded = items[idx].expanded;
    items[idx].expanded = !was_expanded;
    let depth = items[idx].depth;
    let children = items[idx].children.clone();

    if was_expanded {
        // Remove all descendants
        let mut end = idx + 1;
        while end < items.len() && items[end].depth > depth {
            end += 1;
        }
        items.drain(idx + 1..end);
    } else {
        // Insert children
        let new_items = flatten(&children, depth + 1, false);
        let insert_at = idx + 1;
        for (j, item) in new_items.into_iter().enumerate() {
            items.insert(insert_at + j, item);
        }
    }
}

fn weight_bar(w: f32) -> &'static str {
    match (w * 6.0).round() as usize {
        0 => "      ",
        1 => "▌     ",
        2 => "██    ",
        3 => "███   ",
        4 => "████  ",
        5 => "█████ ",
        _ => "██████",
    }
}
