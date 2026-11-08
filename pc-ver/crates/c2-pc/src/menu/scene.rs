//! The menus as a `Scene`: input dispatch, drawing and the next scene.

use super::*;

impl Scene for Menu {
    fn title(&self) -> String {
        "Carnivores 2 (c2rs)".into()
    }

    fn wants_text(&self) -> bool {
        matches!(self.screen, Screen::Register)
    }

    fn update(&mut self, _gpu: &Gpu, _dt: f32, input: &Input) {
        let (s, ox, oy) = self.canvas();
        self.cursor = input.cursor.map(|(x, y)| ((x - ox) / s, (y - oy) / s));
        let p = self.cursor;
        match self.screen {
            Screen::Register => {
                let r = self.reg.region(p);
                self.update_register(input, r);
            }
            Screen::Waiver => {
                let r = self.waiver.region(p);
                self.update_waiver(input, r);
            }
            Screen::Delete => {
                let r = self.delete.region(p);
                self.update_delete(input, r);
            }
            Screen::Main => {
                let r = self.main_board().region(p);
                self.update_main(input, r);
            }
            Screen::Quit => {
                let r = self.quit_board.region(p);
                if (input.clicked && r == 1) || input.pressed(KeyCode::KeyY) {
                    self.quit = true;
                } else if (input.clicked && r == 2)
                    || input.pressed(KeyCode::KeyN)
                    || input.pressed(KeyCode::Escape)
                {
                    self.screen = Screen::Main;
                }
            }
            Screen::Credits => {
                if input.clicked || input.pressed(KeyCode::Escape) || input.pressed(KeyCode::Enter)
                {
                    self.screen = Screen::Main;
                }
            }
            Screen::Setup if self.tranq_ask => self.update_tranq_ask(input),
            Screen::Setup if self.mobile.is_some() => self.update_mobile(input),
            Screen::Setup => {
                let r = self.setup.region(p);
                self.update_setup(input, r);
            }
            Screen::Options => {
                let r = self.options.region(p);
                self.update_options(input, r);
            }
            Screen::Loading => {}
        }
    }

    fn clear_color(&self) -> [f64; 3] {
        [0.0; 3]
    }

    fn draw(&mut self, gpu: &Gpu, pass: &mut wgpu::RenderPass<'_>, screen: (u32, u32)) {
        self.screen_size = screen;
        let mut out = Vec::new();
        let p = self.cursor;
        match self.screen {
            Screen::Register => self.draw_register(&mut out, self.reg.region(p)),
            Screen::Waiver => self.board(&mut out, &self.waiver, &[self.waiver.region(p)]),
            Screen::Delete => {
                self.board(&mut out, &self.delete, &[self.delete.region(p)]);
                if let Some(name) = self.picked.and_then(|i| self.players.get(i)) {
                    let t = format!("Delete {}?", name.player_name());
                    let x = 400.0 - self.text_width(14.0, &t) / 2.0;
                    self.text(&mut out, x, 388.0, 14.0, BLUE, &t);
                }
            }
            Screen::Main => self.draw_main(&mut out, self.main_board().region(p)),
            Screen::Quit => {
                let r = self.quit_board.region(p);
                self.board(&mut out, &self.quit_board, &[r]);
            }
            Screen::Credits => self.board(&mut out, &self.credits, &[]),
            Screen::Setup if self.mobile.is_some() => self.draw_mobile(&mut out),
            Screen::Setup => self.draw_setup(&mut out, self.setup.region(p)),
            Screen::Options => self.draw_options(&mut out, self.options.region(p)),
            Screen::Loading => {
                self.draw_loading(&mut out);
                self.shown_loading = true;
            }
        }
        if self.tranq_ask && matches!(self.screen, Screen::Setup) {
            self.draw_tranq_ask(&mut out);
        }
        self.hud.draw(gpu, pass, screen, &out);
    }

    fn handles_escape(&self) -> bool {
        true
    }

    fn wants_exit(&self) -> bool {
        self.quit
    }

    fn next(&mut self) -> Option<MakeScene> {
        if !self.shown_loading {
            return None;
        }
        let setup = self.launch.take()?;
        Some(Box::new(move |gpu| {
            Ok(Box::new(HuntView::load(gpu, setup)?) as Box<dyn Scene>)
        }))
    }
}
