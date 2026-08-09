use crate::{TreeNode, Weight};

pub fn render_html(nodes: &[TreeNode], cmd: &str, weight: Weight) -> String {
    let tree_html = nodes_to_html(nodes);
    let weight_label = format!("{:?}", weight).to_lowercase();
    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<title>borescope — {cmd}</title>
<style>
body {{ font-family: monospace; background: #1a1a1a; color: #e0e0e0; margin: 2rem; }}
ul {{ list-style: none; padding-left: 1.2rem; }}
li {{ margin: 2px 0; }}
.toggle {{ cursor: pointer; user-select: none; }}
.toggle::before {{ content: "▾ "; }}
.collapsed > ul {{ display: none; }}
.collapsed > .toggle::before {{ content: "▸ "; }}
.added {{ color: #4caf50; }}
.removed {{ color: #f44336; }}
.modified {{ color: #ff9800; }}
.low-conf {{ opacity: 0.55; }}
.bar {{ display: inline-block; background: #4a90d9; height: 10px; vertical-align: middle; margin-left: 4px; }}
.weight {{ font-size: 0.8em; color: #888; margin-left: 4px; }}
h1 {{ font-size: 1.1rem; color: #4a90d9; }}
.controls {{ margin-bottom: 1rem; }}
button {{ background: #333; color: #e0e0e0; border: 1px solid #555; padding: 4px 10px; cursor: pointer; }}
button:hover {{ background: #444; }}
</style>
</head>
<body>
<h1>borescope {cmd} · weight: {weight_label}</h1>
<div class="controls">
  <button onclick="expandAll()">Expand all</button>
  <button onclick="collapseAll()">Collapse all</button>
</div>
<ul id="root">
{tree_html}
</ul>
<script>
function expandAll() {{
  document.querySelectorAll('li').forEach(l => l.classList.remove('collapsed'));
}}
function collapseAll() {{
  document.querySelectorAll('li').forEach(l => {{
    if (l.querySelector('ul')) l.classList.add('collapsed');
  }});
}}
document.querySelectorAll('.toggle').forEach(el => {{
  el.addEventListener('click', () => el.parentElement.classList.toggle('collapsed'));
}});
</script>
</body>
</html>"#,
        cmd = html_escape(cmd),
        weight_label = html_escape(&weight_label),
        tree_html = tree_html,
    )
}

fn nodes_to_html(nodes: &[TreeNode]) -> String {
    nodes.iter().map(node_to_html).collect()
}

fn node_to_html(node: &TreeNode) -> String {
    let mark_class = match node.mark.as_deref() {
        Some("+") => " class=\"added\"",
        Some("-") => " class=\"removed\"",
        Some("~") => " class=\"modified\"",
        _ => "",
    };
    let conf_class = if node.confidence < 0.7 {
        " low-conf"
    } else {
        ""
    };
    let bar_width = (node.weight * 80.0).round() as u32;
    let bar_html = if node.weight > 0.0 {
        format!(
            r#"<span class="bar" style="width:{}px"></span><span class="weight">{:.2}</span>"#,
            bar_width, node.weight
        )
    } else {
        String::new()
    };

    if node.children.is_empty() {
        format!(
            "<li><span{}{}>{}</span>{}</li>\n",
            mark_class,
            if conf_class.is_empty() {
                String::new()
            } else {
                format!(" class=\"{}\"", conf_class.trim())
            },
            html_escape(&node.name),
            bar_html
        )
    } else {
        let children_html = nodes_to_html(&node.children);
        format!(
            "<li><span class=\"toggle{}{}\"{}>{}</span>{}<ul>\n{}</ul></li>\n",
            mark_class,
            conf_class,
            mark_class,
            html_escape(&node.name),
            bar_html,
            children_html
        )
    }
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
