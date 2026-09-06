use super::*;
use kyoku::review::DecisionPoint;

pub(super) fn conversation(
    points: &[DecisionPoint],
    config: Option<&AgentConfig<'_>>,
    mut input: impl BufRead,
    mut output: impl Write,
    mut diagnostics: impl Write,
    show_prompt: bool,
) -> io::Result<()> {
    decision_output::write_decisions(&mut output, points)?;
    if points.is_empty() {
        return Ok(());
    }
    writeln!(
        output,
        "/select N 选择事件；/next、/prev 切换；/show 查看局面；/list 列表；/evidence 工具证据；/quit 退出。直接输入问题进行复盘。"
    )?;
    let mut selected = 0;
    let mut session: Option<AgentSession> = None;
    review_output::write_review(&mut output, &points[selected].review)?;
    loop {
        if show_prompt {
            write!(output, "\nG{:03} 你> ", points[selected].review.event_index)?;
            output.flush()?;
        }
        let mut line = String::new();
        if input.read_line(&mut line)? == 0 {
            return Ok(());
        }
        let command = line.trim();
        match command {
            "" => continue,
            "/quit" => return Ok(()),
            "/list" => decision_output::write_decisions(&mut output, points)?,
            "/show" => review_output::write_review(&mut output, &points[selected].review)?,
            "/evidence" => writeln!(output, "{:#}", review_evidence(&points[selected].review))?,
            "/next" | "/prev" => {
                let next = if command == "/next" {
                    selected
                        .checked_add(1)
                        .filter(|index| *index < points.len())
                } else {
                    selected.checked_sub(1)
                };
                if let Some(next) = next {
                    selected = next;
                    session = None;
                    writeln!(output, "已切换局面，问答上下文已重置。")?;
                    review_output::write_review(&mut output, &points[selected].review)?;
                } else {
                    writeln!(diagnostics, "已到决策列表边界。")?;
                }
            }
            value if value.split_whitespace().next() == Some("/select") => {
                let mut parts = value.split_whitespace().skip(1);
                let event = parts.next().and_then(|part| part.parse::<usize>().ok());
                let next = event.filter(|_| parts.next().is_none()).and_then(|event| {
                    points
                        .binary_search_by_key(&event, |point| point.review.event_index)
                        .ok()
                });
                if let Some(next) = next {
                    if next != selected {
                        selected = next;
                        session = None;
                        writeln!(output, "已切换局面，问答上下文已重置。")?;
                    }
                    review_output::write_review(&mut output, &points[selected].review)?;
                } else {
                    writeln!(
                        diagnostics,
                        "用 /select N 输入列表中的全局事件编号；当前局面保持不变。"
                    )?;
                }
            }
            value if value.starts_with('/') => {
                writeln!(
                    diagnostics,
                    "未知命令；可用 /list、/select N、/next、/prev、/show、/evidence、/quit。"
                )?;
            }
            question => {
                writeln!(diagnostics, "正在请求复盘解释…")?;
                match ask_question(
                    &mut session,
                    &AgentContext::from(&points[selected].review),
                    config,
                    question,
                ) {
                    Ok(answer) => writeln!(output, "{answer}")?,
                    Err(error) => {
                        writeln!(diagnostics, "agent: {error}；本轮未写入会话，可重试。")?
                    }
                }
            }
        }
    }
}
