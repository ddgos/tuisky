use crate::components::log::LogComponent;
use crate::components::main::MainComponent;
use crate::components::Component;
use crate::tui::{io, Tui};
use crate::types::{Action, Event};
use color_eyre::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Layout};
use ratatui::Terminal;
use tokio::sync::mpsc;

pub struct App {
    frame_rate: f64,
    main_component: MainComponent,
    components: Vec<Box<dyn Component>>,
}

impl App {
    pub fn new(frame_rate: f64) -> Self {
        log::debug!("App::new(frame_rate: {frame_rate})");
        Self {
            frame_rate,
            main_component: MainComponent::new(),
            components: Vec::new(),
        }
    }
    pub async fn run(&mut self) -> Result<()> {
        let (action_tx, mut action_rx) = mpsc::unbounded_channel();

        let terminal = Terminal::new(CrosstermBackend::new(io()))?;
        log::debug!("terminal size: {}", terminal.size()?);
        let mut tui = Tui::new(terminal);
        tui.start(self.frame_rate)?;

        self.main_component
            .register_action_handler(action_tx.clone())?;
        for component in self.components.iter_mut() {
            component.register_action_handler(action_tx.clone())?;
        }
        // TODO: config handler?
        self.main_component.init_async(tui.size()?).await?;
        self.main_component.init(tui.size()?)?;
        for component in self.components.iter_mut() {
            component.init(tui.size()?)?;
        }

        let mut should_quit = false;
        loop {
            if let Some(e) = tui.next_event().await {
                if let Some(action) = self.handle_events(e.clone()) {
                    action_tx.send(action)?;
                }
                if let Some(action) = self.main_component.handle_events(Some(e.clone()))? {
                    action_tx.send(action)?;
                }
                for component in self.components.iter_mut() {
                    if let Some(action) = component.handle_events(Some(e.clone()))? {
                        action_tx.send(action)?;
                    }
                }
            }
            while let Ok(action) = action_rx.try_recv() {
                if !matches!(action, Action::Tick(_) | Action::Render) {
                    log::info!("Action {action:?}");
                }
                match action {
                    Action::Quit => should_quit = true,
                    Action::Tick(i) => {
                        log::debug!("tick {i}");
                        if i % 10 == 0 {
                            self.save().await?;
                        }
                    }
                    Action::Render => {
                        tui.draw(|f| {
                            // split horizontally, the right side is for log view
                            let layout = Layout::default()
                                .direction(ratatui::layout::Direction::Horizontal)
                                .constraints([Constraint::Fill(1), Constraint::Max(75)])
                                .split(f.size());
                            // render main components to the left side
                            if let Err(e) = self.main_component.draw(f, layout[0]) {
                                action_tx
                                    .send(Action::Error(format!("failed to draw: {e:?}")))
                                    .expect("failed to send error");
                            }
                            // render log components to the right side
                            if let Err(e) = LogComponent.draw(f, layout[1]) {
                                action_tx
                                    .send(Action::Error(format!("failed to draw: {e:?}")))
                                    .expect("failed to send error");
                            }
                            // other components?
                            for component in self.components.iter_mut() {
                                if let Err(e) = component.draw(f, layout[0]) {
                                    action_tx
                                        .send(Action::Error(format!("failed to draw: {e:?}")))
                                        .expect("failed to send error");
                                }
                            }
                        })?;
                    }
                    _ => {
                        if let Some(action) = self.main_component.update(action.clone())? {
                            action_tx.send(action)?;
                        }
                        for component in self.components.iter_mut() {
                            if let Some(action) = component.update(action.clone())? {
                                action_tx.send(action)?;
                            }
                        }
                    }
                }
            }
            if should_quit {
                break self.save().await?;
            }
        }
        tui.end()?;
        Ok(())
    }
    async fn save(&self) -> Result<()> {
        self.main_component.save().await
    }
    fn handle_events(&mut self, event: Event) -> Option<Action> {
        match event {
            Event::Tick(i) => return Some(Action::Tick(i)),
            Event::Render => return Some(Action::Render),
            Event::Key(key_event) => {
                if let Some(action) = self.handle_key_events(key_event) {
                    return Some(action);
                }
            }
            _ => {}
        }
        None
    }
    fn handle_key_events(&mut self, key_event: KeyEvent) -> Option<Action> {
        if matches!(key_event.code, KeyCode::Char('c' | 'q'))
            && key_event.modifiers == KeyModifiers::CONTROL
        {
            return Some(Action::Quit);
        }
        None
    }
}
