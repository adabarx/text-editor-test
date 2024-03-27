use std::{cmp::min, sync::Arc, u16};

use async_trait::async_trait;
use either::*;
use tokio::{sync::RwLock, task::JoinSet};
use ratatui::{
    buffer::Buffer, layout::{Alignment, Position, Rect}, style::{Color, Style}, widgets::{Clear, WidgetRef}
};

use crate::ARW;

#[async_trait]
pub trait AsyncWidget {
    async fn async_render(&self) -> impl WidgetRef;
    async fn highlight(&self);
    async fn no_highlight(&self);
}

#[derive(Default, Clone, PartialEq, Eq)]
pub struct Char {
    pub char: char,
    pub style: Style,
}

#[derive(Default, Clone)]
pub struct Span {
    pub characters: Vec<ARW<Char>>,
    pub style: Style,
}

#[derive(Default, Clone)]
pub struct Line {
    pub spans: Vec<ARW<Span>>,
    pub style: Style,
}

#[derive(Default, Clone)]
pub struct Text {
    pub scroll_offset: usize,
    pub height: usize,
    pub lines: Vec<ARW<Line>>,
    pub style: Style,
    pub alignment: Option<Alignment>,
}

#[derive(Clone, Copy, Default)]
pub enum SplitDirection {
    #[default]
    Vertical,
    Horizontal,
}

#[derive(Clone)]
pub struct Root {
    pub area: Rect,
    pub style: Style,
    pub split_dir: SplitDirection,
    pub children: Vec<ARW<Window>>,
}

impl Root {
    pub fn new(split_dir: SplitDirection, area: Rect) -> Self {
        Self {
            split_dir,
            area,
            children: Vec::new(),
            style: Style::default()
        }
    }

    pub fn add_window(&mut self, split_dir: SplitDirection, index: usize) {
        let new_child = Arc::new(RwLock::new(Window::new(split_dir, self.area)));
        if index >= self.children.len() {
            self.children.push(Arc::new(RwLock::new(Window::new(split_dir, self.area))));
        }
        let mut children = self.children.clone();
        let mut tmp = children.drain(index..).collect();
        children.push(new_child);
        children.append(&mut tmp);
        self.children = children;
    }
}

#[derive(Clone)]
pub struct Window {
    pub area: Rect,
    pub style: Style,
    pub split_dir: SplitDirection,
    pub children: Vec<Either<ARW<Window>, ARW<Text>>>,
}

impl Window {
    pub fn new(split_dir: SplitDirection, area: Rect) -> Self {
        Self {
            split_dir,
            area,
            children: Vec::new(),
            style: Style::default()
        }
    }

    pub fn add_window(&mut self, split_dir: SplitDirection, index: usize) {
        if index >= self.children.len() {
            self.children.push(Left(Arc::new(RwLock::new(Window::new(split_dir, self.area)))));
        }
        let mut children = self.children.clone();
        let mut tmp = children.drain(index..).collect();
        children.push(Left(Arc::new(RwLock::new(Window::new(split_dir, self.area)))));
        children.append(&mut tmp);
        self.children = children;
    }

    pub fn add_text(&mut self, content: String, index: usize) {
        let mut text = Text::raw(content);
        text.height = self.area.height.into();
        if index >= self.children.len() {
            self.children.push(Right(Arc::new(RwLock::new(text))));
        } else {
            let mut children = self.children.clone();
            let mut tmp = children.drain(index..).collect();
            children.push(Right(Arc::new(RwLock::new(text))));
            children.append(&mut tmp);

        }
    }
}

impl Span { 
    pub fn raw<T: Into<String>>(content: T) -> Span {
        let content: String = content.into();
        Span {
            characters: content
                .chars()
                .map(|ch| Arc::new(RwLock::new(
                    Char { char: ch, style: Style::default() }
                )))
                .collect(),
            style: Style::default(),
        }
    }

    pub async fn is_newline(&self) -> bool {
        if self.characters.len() == 1
            && self.characters[0].read().await.char == b'\n' as char
        {
            return true;
        }
        false

    }
}

impl Text {
    pub fn raw(input: String) -> Text {
        Text {
            lines: input
                .split_inclusive('\n')
                .map(|ln| Arc::new(RwLock::new(Line::raw(ln))))
                .collect(),
            ..Default::default()
        }
    }

    pub fn add_line(&mut self, line: ARW<Line>, index: usize) {
        let len = self.lines.len();
        let mut lines: Vec<ARW<Line>> =
            self.lines
                .drain(min(index, len)..len)
                .collect();

        self.lines.push(line);
        self.lines.append(&mut lines);

        self.lines = lines;
    }

    pub fn get_line(&self, index: usize) -> ARW<Line> {
        self.lines.get(index)
            .unwrap_or(
                self.lines.get(self.lines.len() - 1).unwrap()
            ).clone()
    }
}

impl Line {
    pub fn raw<T: Into<String>>(input: T) -> Line {
        let spans: String = input.into();
        Line {
            spans: spans
                .split_inclusive(' ')
                .map(|sp| Arc::new(RwLock::new(Span::raw(sp))))
                .collect(),
            ..Default::default()
        }
    }
    pub fn add_span(&mut self, span: ARW<Span>, index: usize) {
        let len = self.spans.len();
        let mut spans: Vec<ARW<Span>> =
            self.spans
                .drain(min(index, len)..len)
                .collect();

        self.spans.push(span);
        self.spans.append(&mut spans);

        self.spans = spans;
    }

    pub async fn char_len(&self) -> u16 {
        let mut set = JoinSet::new();

        for sp in self.spans.iter().cloned() {
            set.spawn(async move { sp.read().await.characters.len() });
        }

        let mut count = 0;
        while let Some(Ok(len)) = set.join_next().await {
            count += len;
        }
        count as u16
    }
}

#[async_trait]
impl AsyncWidget for ARW<Char> {
    async fn async_render(&self) -> Char {
        self.read().await.clone()
    }

    async fn highlight(&self) {
        let mut wg = self.write().await;
        wg.style.bg = Some(Color::White);
        wg.style.fg = Some(Color::Black);
    }
    async fn no_highlight(&self) {
        self.write().await.style = Style::default();
    }
}

#[async_trait]
impl AsyncWidget for ARW<Span> {
    async fn async_render(&self) -> SpanRender {
        let mut set = JoinSet::new();
        for (i, char) in self.read().await.characters.iter().cloned().enumerate() {
            set.spawn(async move { (i, char.async_render().await) });
        }
        let mut characters = Vec::new();
        while let Some(Ok(char)) = set.join_next().await {
            characters.push(char);
        }
        characters.sort_by(|a, b| a.0.cmp(&b.0));

        SpanRender {
            characters: characters.into_iter().map(|(_, render)| render).collect(),
            style: self.read().await.style,
            ..Default::default()
        }
    }
    async fn highlight(&self) {
        let mut wg = self.write().await;
        wg.style.bg = Some(Color::White);
        wg.style.fg = Some(Color::Black);
    }
    async fn no_highlight(&self) {
        self.write().await.style = Style::default();
    }
}

#[async_trait]
impl AsyncWidget for ARW<Line> {
    async fn async_render(&self) -> LineRender {
        let mut set = JoinSet::new();
        for (i, span) in self.read().await.spans.iter().cloned().enumerate() {
            set.spawn(async move { (i, span.async_render().await) });
        }
        let mut spans = Vec::new();
        while let Some(Ok(span)) = set.join_next().await {
            spans.push(span);
        }
        spans.sort_by(|a, b| a.0.cmp(&b.0));

        LineRender {
            spans: spans.into_iter().map(|(_, render)| render).collect(),
            style: self.read().await.style,
            ..Default::default()
        }
    }

    async fn highlight(&self) {
        let mut wg = self.write().await;
        wg.style.bg = Some(Color::White);
        wg.style.fg = Some(Color::Black);
    }
    async fn no_highlight(&self) {
        self.write().await.style = Style::default();
    }
}

#[async_trait]
impl AsyncWidget for ARW<Text> {
    async fn async_render(&self) -> TextRender {
        let mut set = JoinSet::new();
        let offset = self.read().await.scroll_offset;
        let height = self.read().await.height;
        for (i, line) in self.read().await.lines.iter().cloned().enumerate() {
            if i >= offset && i < offset + height {
                set.spawn(async move { (i, line.async_render().await) });
            }
        }

        let mut lines = Vec::new();
        while let Some(Ok(line)) = set.join_next().await {
            lines.push(line);
        }
        lines.sort_by(|a, b| a.0.cmp(&b.0));

        TextRender { 
            top: self.read().await.scroll_offset.clone(),
            lines: lines.into_iter().map(|(_, render)| render).collect(),
            style: self.read().await.style.clone(),
            alignment: self.read().await.alignment.clone(),
        }
    }
    async fn highlight(&self) {
        let mut wg = self.write().await;
        wg.style.bg = Some(Color::White);
        wg.style.fg = Some(Color::Black);
    }
    async fn no_highlight(&self) {
        self.write().await.style = Style::default();
    }
}

#[async_trait]
impl AsyncWidget for ARW<Window> {
    async fn async_render(&self) -> WindowRender {
        let snapshot = self.read().await.clone();
        let mut set = JoinSet::new();
        for (i, child) in snapshot.children.iter().cloned().enumerate() {
            set.spawn(async move { 
                match child {
                    Left(window) => (i, Left(window.async_render().await)),
                    Right(text) => (i, Right(text.async_render().await)),
                }
            });
        }
        let mut children = Vec::new();
        while let Some(Ok(line)) = set.join_next().await {
            children.push(line);
        }
        children.sort_by(|a, b| a.0.cmp(&b.0));
        WindowRender {
            style: snapshot.style,
            split_dir: snapshot.split_dir,
            children: children.into_iter().map(|c| c.1).collect()
        }
    }

    async fn highlight(&self) {
        let mut wg = self.write().await;
        wg.style.bg = Some(Color::White);
        wg.style.fg = Some(Color::Black);
    }
    async fn no_highlight(&self) {
        self.write().await.style = Style::default();
    }
}

#[async_trait]
impl AsyncWidget for &'static RwLock<Root> {
    async fn async_render(&self) -> WindowRender {
        let snapshot = self.read().await.clone();
        let len = snapshot.children.len();
        let mut set = JoinSet::new();
        for i in 0..len {
            let child = snapshot.children[i].clone();
            set.spawn(async move { (i, child.async_render().await) });
        }
        let mut children = Vec::with_capacity(snapshot.children.len());
        while let Some(Ok(line)) = set.join_next().await {
            children.push(line);
        }
        children.sort_by(|a, b| a.0.cmp(&b.0));
        WindowRender {
            style: snapshot.style,
            split_dir: snapshot.split_dir,
            children: children.into_iter().map(|c| Left(c.1)).collect()
        }
    }

    async fn highlight(&self) {
        let mut wg = self.write().await;
        wg.style.bg = Some(Color::White);
        wg.style.fg = Some(Color::Black);
    }
    async fn no_highlight(&self) {
        self.write().await.style = Style::default();
    }
}

pub enum LayoutTypeRender {
    Container {
        split_direction: SplitDirection,
        layouts: Vec<WindowRender>,
    },
    Content(TextRender),
}

#[derive(Default)]
pub struct SpanRender {
    pub characters: Vec<Char>,
    pub style: Style,
    pub alignment: Option<Alignment>,
}

#[derive(Default)]
pub struct LineRender {
    pub spans: Vec<SpanRender>,
    pub style: Style,
    pub alignment: Option<Alignment>,
}

#[derive(Default)]
pub struct TextRender {
    pub top: usize,
    pub lines: Vec<LineRender>,
    pub style: Style,
    pub alignment: Option<Alignment>,
}

pub struct WindowRender {
    style: Style,
    split_dir: SplitDirection,
    children: Vec<Either<WindowRender, TextRender>>,
}

impl WidgetRef for Char {
    fn render_ref(&self,area:Rect,buf: &mut Buffer) {
        buf.set_style(area, self.style);
        buf.get_mut(area.x, area.y).set_symbol(&self.char.to_string());
    }
}

impl WidgetRef for SpanRender {
    fn render_ref(&self,area:Rect,buf: &mut Buffer) {
        // height is already 1
        if self.characters.len() == 0 {
            let area = Rect { width: 1, ..area };
            buf.set_style(area, self.style);
            return;
        }
        buf.set_style(area, self.style);
        let mut i: u16 = 0;
        for ch in self.characters.iter() {
            let char_area = Rect {
                x: area.x + i,
                width: 1,
                ..area
            };
            if !area.contains(Position::new(char_area.x, char_area.y)) {
                break
            }
            ch.render_ref(char_area, buf);
            i += 1;
        }
    }
}

impl WidgetRef for LineRender {
    fn render_ref(&self,area:Rect,buf: &mut Buffer) {
        // height is already 1
        let len = self.spans.len();
        if len == 0 {
            let area = Rect { width: 1, ..area };
            buf.set_style(area, self.style);
            return;
        }
        buf.set_style(area, self.style);
        let mut offset: u16 = 0;
        for span in self.spans.iter() {
            let width = span.characters.len() as u16;
            let area = Rect {
                x: area.x + offset,
                y: area.y,
                width,
                height: 1,
            };
            span.render_ref(area, buf);
            offset += width;
        }
    }
}

impl WidgetRef for WindowRender {
    fn render_ref(&self,area:Rect,buf: &mut Buffer) {
        buf.set_style(area, self.style);
        let windows: u16 = self.children.len().try_into().unwrap();
        if windows == 0 { return; }
        match self.split_dir {
            SplitDirection::Horizontal => {
                // split is horizontal. nested containers are stacked vertically
                let offset = area.height / windows;
                for (i, child) in self.children.iter().enumerate() {
                    let area = Rect::new(
                        area.x,
                        if i == 0 { area.y } else { area.y + offset + 1 },
                        area.width,
                        offset
                    );
                    for_both!(child, c => c.render_ref(area, buf))
                }
            },
            SplitDirection::Vertical => {
                // split is vertical. nested containers are stacked horizontally
                let offset = area.width / windows;
                for (i, child) in self.children.iter().enumerate() {
                    let area = Rect::new(
                        if i == 0 { area.x } else { area.x + offset + 1 },
                        area.y,
                        offset,
                        area.height,
                    );
                    for_both!(child, c => c.render_ref(area, buf))
                }
            },
        }
    }
}

impl WidgetRef for TextRender {
    fn render_ref(&self, area: Rect, buf: &mut Buffer) {
        buf.set_style(area, self.style);
        let width = area.width;
        for (i, line) in self.lines.iter().enumerate() {
            let area = Rect {
                x: area.x,
                y: area.y + i as u16,
                width: line.spans
                    .iter()
                    .fold(0_u16, |acc, sp| acc + sp.characters.len() as u16),
                height: 1,
            };
            Clear.render_ref(Rect { width, ..area }, buf);
            line.render_ref(area, buf);
        }
    }
}

