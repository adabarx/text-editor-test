use std::{cell::RefCell, cmp::min, collections::VecDeque, rc::Rc};

use ratatui::style::{Color, Style};
use anyhow::{anyhow, bail, Result};

use crate::{primatives::{Char, Layout, LayoutType, Line, Mother, Span, TryMother}, ARW};

trait Zipper {
    fn prev(self) -> Option<LayoutZipper>;

    fn mother(self) -> MoveResult;
    fn try_add_child(&mut self, child: Node, index: usize) -> Result<()>;
    fn daughter(self, index: usize) -> MoveResult;

    fn left_sister(self) -> MoveResult;
    fn right_sister(self) -> MoveResult;

    fn left_aunt(self) -> MoveResult;
    fn right_aunt(self) -> MoveResult;

    fn left_cousin(self, index: usize) -> MoveResult;
    fn right_cousin(self, index: usize) -> MoveResult;

    fn left_sister_or_cousin(self) -> MoveResult;
    fn right_sister_or_cousin(self) -> MoveResult;

    fn replace_focus(self, new_node: Node) -> LayoutZipper;
}

#[derive(Clone)]
pub enum MoveResult {
    Moved(LayoutZipper),
    DidntMove(LayoutZipper)
}

impl MoveResult {
    pub fn unwrap(self) -> LayoutZipper {
        match self {
            MoveResult::Moved(zip) => zip,
            MoveResult::DidntMove(zip) => zip,
        }
    }

    pub fn inner_mut(&mut self) -> &mut LayoutZipper {
        match self {
            MoveResult::Moved(zip) => zip,
            MoveResult::DidntMove(zip) => zip,
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
enum PrevDir {
    Parent,
    Left,
    Right,
}

#[derive(Clone)]
struct Breadcrumb {
    zipper: Box<LayoutZipper>,
    direction: PrevDir,
}

enum NodeResult {
    Success(Node),
    Failed(Node),
}

#[derive(Clone)]
pub enum Node {
    Layout(ARW<Layout>),
    Line(ARW<Line>),
    Span(ARW<Span>),
    Char(ARW<Char>),
}

impl Node {
    pub fn get_layout(&self) -> Option<ARW<Layout>> {
        if let Node::Layout(layout) = self {
            Some(layout.clone())
        } else {
            None
        }
    }

    fn try_add_child(&mut self, child: Node, index: usize) -> Result<Node> {
        use Node::*;
        match (self, child) {
            (Layout(mom), Layout(child)) => Ok(Node::Layout(
                mom.write().unwrap().try_add_child(child.read().unwrap().clone(), index)?
            )),
            (Layout(mom), Line(child)) => Ok(Node::Line(
                mom.write().unwrap().try_add_child(child.read().unwrap().clone(), index)?
            )),
            (Line(mom), Span(child)) => Ok(Node::Span(
                mom.write().unwrap().add_child(child.read().unwrap().clone(), index)
            )),
            (Span(mom), Char(child)) => Ok(Node::Char(
                mom.write().unwrap().add_child(child.read().unwrap().clone(), index)
            )),
            _ => Err(anyhow!("this child does not please mother")),
        }
    }

    pub fn get_children(&self) -> Option<Vec<Node>> {
        // returns None if node doesn't carry children
        // returns an empty vec if the node can carry
        // children but currently doesn't
        match self {
            Node::Layout(layout) => {
                let layout = layout.read().unwrap().layout.clone();
                Some(match layout {
                    LayoutType::Content(text) => text.lines
                        .iter()
                        .map(|l| Node::Line(l.clone()))
                        .collect(),
                    LayoutType::Container { layouts, .. } => layouts
                        .iter()
                        .map(|l| Node::Layout(l.clone()))
                        .collect(),
                })
            },
            Node::Span(span) => Some(
                span.read().unwrap().characters
                    .iter()
                    .map(|ch| Node::Char(ch.clone()))
                    .collect()
            ),
            Node::Line(line) => Some(
                line.read().unwrap().spans
                    .iter()
                    .map(|sp| Node::Span(sp.clone()))
                    .collect()
            ),
            Node::Char(_) => None,
        }
    }

    pub fn highlight(&mut self) {
        match self {
            Node::Line(line) => {
                line.write().unwrap().style.bg = Some(Color::White);
                line.write().unwrap().style.fg = Some(Color::Black);
            },
            Node::Span(span) => {
                span.write().unwrap().style.bg = Some(Color::White);
                span.write().unwrap().style.fg = Some(Color::Black);
            },
            Node::Char(ch) => {
                ch.write().unwrap().style.bg = Some(Color::White);
                ch.write().unwrap().style.fg = Some(Color::Black);
            },
            Node::Layout(layout) => {
                layout.write().unwrap().style.bg = Some(Color::White);
                layout.write().unwrap().style.fg = Some(Color::Black);
            },
        }
    }

    pub fn no_highlight(&mut self) {
        match self {
            Node::Line(line) => line.write().unwrap().style = Style::default(),
            Node::Span(span) => span.write().unwrap().style = Style::default(),
            Node::Char(char) => char.write().unwrap().style = Style::default(),
            Node::Layout(layout) => layout.write().unwrap().style = Style::default(),
        }
    }
}

pub struct LayoutCrumb {

}

#[derive(Clone)]
pub struct LayoutZipper {
    previous: Option<Breadcrumb>,
    focus: Node,
    children: Vec<Node>,
    left: Vec<Node>,
    right: VecDeque<Node>,
}

impl LayoutZipper {
    pub fn new(root: Node) -> Self {
        let children = root.get_children().unwrap();
        Self {
            focus: root,
            children,
            previous: None,
            left: Vec::new(),
            right: VecDeque::new(),
        }
    }

    pub fn try_add_child(&mut self, child: Node, index: usize) -> Result<()> {
        self.focus.try_add_child(child.clone(), index).unwrap();

        let len = self.children.len();
        let mut children: Vec<Node> = self.children.drain(min(index, len)..len).collect();
        self.children.push(child);
        self.children.append(&mut children);
        Ok(())
    }

    pub fn move_to_child(mut self, index: usize) -> MoveResult {
        self.focus.no_highlight();
        let mut result = self.daughter(index);
        result.inner_mut().focus.highlight();
        result
    }

    pub fn move_to_prev(mut self) -> Option<LayoutZipper> {
        self.focus.no_highlight();
        let mut rv = self.prev().unwrap();
        rv.focus.highlight();
        Some(rv)
    }

    pub fn try_move_right(mut self) -> MoveResult {
        self.focus.no_highlight();
        let mut result = self.right_sister();
        result.inner_mut().focus.highlight();
        result
    }

    pub fn try_move_left(mut self) -> MoveResult {
        self.focus.no_highlight();
        let mut result = self.left_sister();
        result.inner_mut().focus.highlight();
        result
    }

    pub fn move_left_catch_ignore(self) -> LayoutZipper {
        self.try_move_right().unwrap()
    }

    pub fn move_right_catch_ignore(self) -> LayoutZipper {
        self.try_move_right().unwrap()
    }

    pub fn go_back_to_parent(mut self) -> MoveResult {
        self.focus.no_highlight();
        let mut result = self.mother();
        result.inner_mut().focus.highlight();
        result
    }

    pub fn move_right_or_cousin(mut self) -> MoveResult {
        self.focus.no_highlight();
        let mut result = self.right_sister_or_cousin();
        result.inner_mut().focus.highlight();
        result
    }

    pub fn move_left_or_cousin(mut self) -> MoveResult {
        self.focus.no_highlight();
        let mut result = self.left_sister_or_cousin();
        result.inner_mut().focus.highlight();
        result
    }

    pub fn add_child(mut self, node: Node, index: usize) -> LayoutZipper {
        let len = self.children.len();
        if index >= len {
            self.children.push(node);
            return self;
        }

        let mut children = self.children[0..index].to_vec();
        let mut child = vec![node];
        let mut the_rest = self.children[index + 1..len].to_vec();
        children.append(&mut child);
        children.append(&mut the_rest);

        self.children = children;
        self
    }

    pub fn replace_focus(mut self, new_node: Node) -> LayoutZipper {
        self.children = new_node.get_children().unwrap_or(Vec::new());
        self.focus = new_node;
        self
    }
}

impl Zipper for MoveResult {
    fn mother(self) -> MoveResult {
        if let MoveResult::DidntMove(_) = self { return self }
        self.unwrap().mother()
    }

    fn try_add_child(&mut self, child: Node, index: usize) -> Result<()> {
        self.unwrap().try_add_child(child, index)
    }

    fn replace_focus(self, new_node: Node) -> LayoutZipper {
        if let MoveResult::DidntMove(_) = self { return self.unwrap() }
        self.unwrap().replace_focus(new_node)
    }    

    fn right_aunt(self) -> MoveResult {
        if let MoveResult::DidntMove(_) = self { return self }
        self.unwrap().right_aunt()
    }

    fn left_aunt(self) -> MoveResult {
        if let MoveResult::DidntMove(_) = self { return self }
        self.unwrap().left_aunt()
    }

    fn right_cousin(self, index: usize) -> MoveResult {
        if let MoveResult::DidntMove(_) = self { return self }
        self.unwrap().right_cousin(index)
    }

    fn left_cousin(self, index: usize) -> MoveResult {
        if let MoveResult::DidntMove(_) = self { return self }
        self.unwrap().left_cousin(index)
    }

    fn daughter(self, index: usize) -> MoveResult {
        if let MoveResult::DidntMove(_) = self { return self }
        self.unwrap().daughter(index)
    }

    fn left_sister(self) -> MoveResult {
        if let MoveResult::DidntMove(_) = self { return self }
        self.unwrap().left_sister()
    }

    fn right_sister(self) -> MoveResult {
        if let MoveResult::DidntMove(_) = self { return self }
        self.unwrap().right_sister()
    }

    fn left_sister_or_cousin(self) -> MoveResult {
        if let MoveResult::DidntMove(_) = self { return self }
        self.unwrap().left_sister_or_cousin()
    }

    fn right_sister_or_cousin(self) -> MoveResult {
        if let MoveResult::DidntMove(_) = self { return self }
        self.unwrap().right_sister_or_cousin()
    }

    fn prev(self) -> Option<LayoutZipper> {
        if let MoveResult::DidntMove(_) = self { return None }
        self.unwrap().prev()
    }

}

impl Zipper for LayoutZipper {
    fn mother(self) -> MoveResult {
        if self.previous.is_none() { return MoveResult::DidntMove(self) }
        let prev = self.previous.unwrap();
        match prev.direction {
            PrevDir::Parent => MoveResult::Moved(*prev.zipper),
            PrevDir::Left => prev.zipper.mother(),
            PrevDir::Right => prev.zipper.mother(),
        }
    }

    fn try_add_child(&mut self, child: Node, index: usize) -> Result<()> {
        let children = &mut self.children;
        let tail = &mut children.drain(index..).collect();
        children.push(child);
        children.append(tail);
        Ok(())
    }

    fn right_aunt(self) -> MoveResult {
        let og = self.clone();
        let result = self.mother();
        if let MoveResult::DidntMove(_) = result {
            return MoveResult::DidntMove(og);
        }
        let result = result.unwrap().right_sister();
        if let MoveResult::DidntMove(_) = result {
            return MoveResult::DidntMove(og);
        }
        result
    }

    fn left_aunt(self) -> MoveResult {
        let og = self.clone();
        let result = self.mother();
        if let MoveResult::DidntMove(_) = result {
            return MoveResult::DidntMove(og);
        }
        let result = result.unwrap().left_sister();
        if let MoveResult::DidntMove(_) = result {
            return MoveResult::DidntMove(og);
        }
        result
    }

    fn right_cousin(self, index: usize) -> MoveResult {
        let og = self.clone();
        let result = self.right_aunt();
        if let MoveResult::DidntMove(_) = result {
            return MoveResult::DidntMove(og);
        }
        let result = result.unwrap().daughter(index);
        if let MoveResult::DidntMove(_) = result {
            return MoveResult::DidntMove(og);
        }
        result
    }

    fn left_cousin(self, index: usize) -> MoveResult {
        let og = self.clone();
        let result = self.left_aunt();
        if let MoveResult::DidntMove(_) = result {
            return MoveResult::DidntMove(og);
        }
        let result = result.unwrap().daughter(index);
        if let MoveResult::DidntMove(_) = result {
            return MoveResult::DidntMove(og);
        }
        result
    }

    fn daughter(self, mut index: usize) -> MoveResult {
        let len = self.children.len();
        if len == 0 { return MoveResult::DidntMove(self) }
        if index >= len { 
            index = len - 1;
        }

        let left = self.children[0..index]
            .iter()
            .cloned()
            .collect();
        let right = self.children[index + 1..len]
            .iter()
            .cloned()
            .collect();
        let focus = self.children[index].clone();
        let children = focus.get_children().unwrap_or(Vec::new());
        let previous = Some(Breadcrumb { zipper: Box::new(self), direction: PrevDir::Parent });
        
        MoveResult::Moved(LayoutZipper { previous, focus, children, left, right })
    }

    fn left_sister(self) -> MoveResult {
        if let Some(prev) = self.previous.as_ref() {
            if prev.direction == PrevDir::Left {
                return MoveResult::Moved(self.move_to_prev().unwrap());
            }
        }

        let mut left = self.left.clone();
        let focus = if let Some(node) = left.pop() { node }
            else { return MoveResult::DidntMove(self); };

        let mut right = self.right.clone();
        right.push_front(self.focus.clone());
        let children = focus.get_children().unwrap_or(Vec::new());
        let previous = Some(Breadcrumb { zipper: Box::new(self), direction: PrevDir::Right });

        MoveResult::Moved(LayoutZipper { focus, previous, children, left, right })
    }

    fn right_sister(self) -> MoveResult {
        if let Some(prev) = self.previous.as_ref() {
            if prev.direction == PrevDir::Right {
                return MoveResult::Moved(self.move_to_prev().unwrap());
            }
        }

        let mut right = self.right.clone();
        let focus = if let Some(node) = right.pop_front() { node }
            else { return MoveResult::DidntMove(self); };

        let mut left = self.left.clone();
        left.push(self.focus.clone());
        let children = focus.get_children().unwrap_or(Vec::new());
        let previous = Some(Breadcrumb { zipper: Box::new(self), direction: PrevDir::Left });

        MoveResult::Moved(LayoutZipper { focus, previous, children, left, right })
    }

    fn left_sister_or_cousin(self) -> MoveResult {
        let og = self.clone();
        let sister = self.left_sister();
        if let MoveResult::Moved(_) = sister {
            return sister;
        }
        og.left_cousin(usize::MAX)
    }

    fn right_sister_or_cousin(self) -> MoveResult {
        let og = self.clone();
        let sister = self.right_sister();
        if let MoveResult::Moved(_) = sister {
            return sister;
        }
        og.right_cousin(0)
    }

    fn prev(self) -> Option<LayoutZipper> {
        if self.previous.is_none() { return None }
        Some(*self.previous.unwrap().zipper)
    }

    fn replace_focus(mut self, new_node: Node) -> LayoutZipper {
        self.children = new_node.get_children().unwrap_or(Vec::new()).clone();
        self.focus = new_node.clone();
        self
    }
}
