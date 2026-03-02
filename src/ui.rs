use crate::app_state::{AppStateManager, Progress, Submenu};
use crate::update::do_update;
use egui::{Button, Color32, FullOutput, ProgressBar};
use egui_backend::egui;
use egui_backend::{sdl2::event::Event, DpiScaling, ShaderVersion};
use egui_sdl2_gl as egui_backend;
use egui_sdl2_gl::egui::{
    CornerRadius, FontData, FontDefinitions, FontFamily, RichText, Spinner, Vec2,
};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::{io::Read, sync::Arc, time::Instant};

use crate::{Result, SDCARD_ROOT};

const DEFAULT_WIDTH: u32 = 1024;
const DEFAULT_HEIGHT: u32 = 768;
const REFERENCE_HEIGHT: f32 = 240.0; // Reference height unit for scaling (240px = 1x scale)
const DEFAULT_DPI_SCALE: f32 = 4.0 / 3.0; // Adjusted for new 240px reference (was 4.0 at 768px = 3x)
const REFERENCE_DPI: f32 = 96.0; // Standard screen DPI reference
const FONTS: [&str; 2] = ["BPreplayBold-unhinted.otf", "chillroundm.ttf"];

// Runtime-calculated scaling based on actual screen DPI
static mut DPI_SCALE_FACTOR: f32 = 1.0;

// Helper function to scale UI values based on display DPI
fn scale(value: f32) -> f32 {
    value //* unsafe { DPI_SCALE_FACTOR }
}

// Helper function to create default text (size is set in style)
fn text(s: impl Into<String>) -> RichText {
    RichText::new(s)
}

#[allow(clippy::too_many_lines)]
fn nextui_ui(ui: &mut egui::Ui, app_state: &'static AppStateManager) -> egui::Response {
    let current_version = app_state.current_version();
    let mut latest_release = app_state.nextui_release().clone();
    let mut latest_tag = app_state.nextui_tag().clone();
    let mut update_available = true;
    let latest_discarded = app_state.nextui_tag().clone().is_none();

    if app_state.release_selection_menu() {
        let index = app_state.nextui_releases_and_tags_index().unwrap_or(0);
        let relase_and_tag_vector = app_state.nextui_releases_and_tags().unwrap_or_default();
        let release_and_tag = relase_and_tag_vector.get(index).cloned();
        latest_release = release_and_tag.as_ref().map(|r| r.release.clone());
        latest_tag = release_and_tag.map(|r| r.tag.clone());
    }

    if app_state.release_selection_menu() & !app_state.release_selection_confirmed() {
        ui.add_space(scale(16.0));
        ui.label(text(
            "WARNING\n\
            Downgrades are not fully supported by NextUI!\n\
            Some settings may be lost or unstable in old versions\n\
            Manual editing of settings or files may be required",
        ));
    } else {
        // Show release information if available
        match (current_version, latest_tag, latest_release) {
            (Some(current_version), Some(tag), _) => {
                let selected_tag = hint_wrap_nextui_tag(app_state, &tag.name);
                if tag.commit.sha.starts_with(&current_version) && !latest_discarded {
                    if app_state.release_selection_menu() {
                        // selection view
                        ui.label(
                            text(format!("Selected Version: {selected_tag}\nThis version is currently already installed!")),
                        );
                    } else {
                        ui.label(text(format!(
                            "You currently have the latest available version:\n{selected_tag}"
                        )));
                    }
                    update_available = false;
                } else if app_state.release_selection_menu() {
                    // selection view
                    ui.label(text(format!("Selected Version: {selected_tag}")));
                } else {
                    if let Some(current_name) = app_state.current_version_name() {
                        ui.label(text(format!("Current version: {current_name}")));
                    }
                    ui.label(text(format!("New version available: {selected_tag}")));
                }
            }
            (_, _, Some(release)) => {
                if app_state.release_selection_menu() {
                    // selection view
                    let selected_tag = hint_wrap_nextui_tag(app_state, &release.tag_name);
                    ui.label(text(format!("Selected Version: {selected_tag}")));
                } else {
                    ui.label(text(format!("Latest version: NextUI {}", release.tag_name)));
                }
            }
            _ => {
                ui.label(text("No release information available"));
            }
        }
    }

    ui.add_space(scale(8.0));

    if app_state.release_selection_menu() & !app_state.release_selection_confirmed() {
        let back_button = ui.button(text("Return"));
        if back_button.clicked() {
            app_state.set_release_selection_menu(false);
        }

        let confirm_button = ui.button(text("Accept Warning"));
        if confirm_button.clicked() {
            app_state.set_release_selection_confirmed(true);
        }

        if back_button.has_focus() {
            app_state.set_hint(Some("Return to Latest Version options".to_string()));
        } else if confirm_button.has_focus() {
            app_state.set_hint(Some("Confirm warning and open update options".to_string()));
        } else {
            app_state.set_hint(None);
        }

        back_button
    } else if update_available {
        let quick_update_button = ui.add(Button::new(text("Quick Update")));

        // Initiate update if button clicked
        if quick_update_button.clicked() {
            // Clear any previous errors
            app_state.set_error(None);
            do_update(app_state, false);
        }

        ui.add_space(scale(4.0));

        let full_update_button = ui.add(Button::new(text("Full Update")));

        if full_update_button.clicked() {
            // Clear any previous errors
            app_state.set_error(None);
            do_update(app_state, true);
        }

        // HINTS
        if quick_update_button.has_focus() {
            app_state.set_hint(Some("Update MinUI.zip only".to_string()));
        } else if full_update_button.has_focus() {
            app_state.set_hint(Some("Extract full zip files (base + extras)".to_string()));
        } else {
            app_state.set_hint(None);
        }

        quick_update_button
    } else {
        let force_button = ui.button(text("Update anyway"));
        if force_button.clicked() {
            app_state.set_nextui_tag(None); // forget the tag
        }

        let quit_button = ui.button(text("Quit"));
        if quit_button.clicked() {
            if app_state.release_selection_menu() {
                app_state.set_release_selection_menu(false);
            } else {
                app_state.set_should_quit(true);
            }
        }

        if quit_button.has_focus() {
            if app_state.release_selection_menu() {
                app_state.set_hint(Some("Return to Latest Version options".to_string()));
            } else {
                app_state.set_hint(Some("Quit NextUI Updater".to_string()));
            }
        } else if force_button.has_focus() {
            app_state.set_hint(Some("Ignore current version".to_string()));
        } else {
            app_state.set_hint(None);
        }

        quit_button
    }
}

// Map controller buttons to keyboard keys
fn controller_to_key(button: sdl2::controller::Button) -> Option<sdl2::keyboard::Keycode> {
    match button {
        sdl2::controller::Button::DPadUp => Some(sdl2::keyboard::Keycode::Up),
        sdl2::controller::Button::DPadDown => Some(sdl2::keyboard::Keycode::Down),
        sdl2::controller::Button::DPadLeft => Some(sdl2::keyboard::Keycode::Left),
        sdl2::controller::Button::DPadRight => Some(sdl2::keyboard::Keycode::Right),
        sdl2::controller::Button::B => Some(sdl2::keyboard::Keycode::Return),
        sdl2::controller::Button::A => Some(sdl2::keyboard::Keycode::Escape),
        sdl2::controller::Button::Y => Some(sdl2::keyboard::Keycode::X),
        _ => None,
    }
}

fn setup_ui_style() -> egui::Style {
    let mut style = egui::Style::default();

    // Scale button spacing and padding
    style.spacing.button_padding = Vec2::new(scale(8.0), scale(2.0));
    style.spacing.interact_size = Vec2::new(scale(8.0), scale(8.0));
    style.spacing.item_spacing = Vec2::new(scale(4.0), scale(4.0));

    style.visuals.panel_fill = Color32::BLACK;
    style.visuals.selection.bg_fill = Color32::WHITE;
    style.visuals.selection.stroke.color = Color32::GRAY;

    style.visuals.widgets.inactive.fg_stroke.color = Color32::WHITE;
    style.visuals.widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;

    style.visuals.widgets.active.bg_fill = Color32::WHITE;
    style.visuals.widgets.active.weak_bg_fill = Color32::WHITE;
    style.visuals.widgets.active.fg_stroke.color = Color32::BLACK;
    style.visuals.widgets.active.corner_radius = CornerRadius::same(255);

    style.visuals.widgets.noninteractive.fg_stroke.color = Color32::WHITE;
    style.visuals.widgets.noninteractive.bg_fill = Color32::TRANSPARENT;

    style.visuals.widgets.hovered.bg_fill = Color32::WHITE;
    style.visuals.widgets.hovered.weak_bg_fill = Color32::TRANSPARENT;
    style.visuals.widgets.hovered.corner_radius = CornerRadius::same(255);

    // Set default text size for all TextStyles
    for (_text_style, font_id) in style.text_styles.iter_mut() {
        font_id.size = scale(10.0);
    }

    style
}

fn init_sdl(
    mock_display_size: Option<(u32, u32)>,
) -> Result<(
    sdl2::Sdl,
    sdl2::video::Window,
    sdl2::EventPump,
    Option<sdl2::controller::GameController>,
)> {
    let sdl_context = sdl2::init()?;
    let video_subsystem = sdl_context.video()?;

    // Get actual screen DPI for scaling
    let dpi = video_subsystem
        .display_dpi(0)
        .unwrap_or((REFERENCE_DPI, REFERENCE_DPI, REFERENCE_DPI))
        .0;

    // Get display bounds for resolution-based scaling fallback
    let (screen_width, screen_height) = if let Some((mock_width, mock_height)) = mock_display_size {
        println!(
            "[DEBUG] Using mock display size: {}x{}",
            mock_width, mock_height
        );
        (mock_width as f32, mock_height as f32)
    } else {
        let display_bounds = video_subsystem.display_bounds(0)?;
        (
            display_bounds.width() as f32,
            display_bounds.height() as f32,
        )
    };

    println!(
        "Screen dimensions: {}x{}, reported DPI: {:.0}",
        screen_width, screen_height, dpi
    );

    // Calculate DPI scale factor
    // If DPI detection doesn't work (returns default 96), use resolution-based estimate
    let dpi_scale = if (dpi - REFERENCE_DPI).abs() < 0.1 {
        // DPI detection likely failed, use resolution-based scaling
        // Use 240px as the reference height unit: 480px = 2x, 768px ≈ 3x, 720px = 3x
        let height_scale = (screen_height / REFERENCE_HEIGHT).round();
        println!(
            "DPI detection unreliable, using resolution-based scaling: {:.1}x ({}px / {}px)",
            height_scale, screen_height, REFERENCE_HEIGHT
        );
        height_scale.max(1.0).min(4.0) // Clamp between 1x and 4x
    } else {
        // DPI detection worked, use it
        println!(
            "Using DPI-based scaling: {:.2}x (screen height: {:.0}px, DPI: {:.0})",
            dpi / REFERENCE_DPI,
            screen_height,
            dpi
        );
        dpi / REFERENCE_DPI
    };

    println!("Final UI scale factor: {:.2}x", dpi_scale);

    unsafe {
        DPI_SCALE_FACTOR = dpi_scale;
    }

    // When mock display size is provided, use it directly
    // Otherwise, scale the default dimensions proportionally
    #[allow(clippy::cast_sign_loss)]
    let (window_width, window_height) = if let Some((mock_width, mock_height)) = mock_display_size {
        // Mock display size is already in the desired resolution
        (mock_width, mock_height)
    } else {
        let width = ((DEFAULT_WIDTH as f32 * unsafe { DPI_SCALE_FACTOR }).max(1.0)) as u32;
        let height = ((DEFAULT_HEIGHT as f32 * unsafe { DPI_SCALE_FACTOR }).max(1.0)) as u32;
        (width, height)
    };

    println!(
        "Creating window with size: {}x{}",
        window_width, window_height
    );

    // Initialize game controller subsystem
    let game_controller_subsystem = sdl_context.game_controller()?;
    let available = game_controller_subsystem.num_joysticks()?;

    // Attempt to open the first available game controller
    let controller = (0..available).find_map(|id| {
        if !game_controller_subsystem.is_game_controller(id) {
            return None;
        }

        match game_controller_subsystem.open(id) {
            Ok(c) => Some(c),
            Err(e) => {
                eprintln!("Failed to open controller {id}: {e:?}");
                None
            }
        }
    });

    // Create a window
    let base_title = format!("NextUI Updater {}", env!("CARGO_PKG_VERSION"));
    let window_title = if let Some((width, height)) = mock_display_size {
        format!(
            "{} - {}x{}, {:.2}x scale",
            base_title, width, height, dpi_scale
        )
    } else {
        base_title
    };

    let window = video_subsystem
        .window(&window_title, window_width, window_height)
        .position_centered()
        .opengl()
        .build()?;

    let event_pump = sdl_context.event_pump()?;

    Ok((sdl_context, window, event_pump, controller))
}

// Load font from file
fn load_font() -> Result<FontDefinitions> {
    fn get_font_preference() -> Result<usize> {
        // Load NextUI settings
        let mut settings_file =
            std::fs::File::open(SDCARD_ROOT.to_owned() + ".userdata/shared/minuisettings.txt")?;

        let mut settings = String::new();
        settings_file.read_to_string(&mut settings)?;

        // Very crappy parser
        Ok(settings.contains("font=1").into())
    }

    // Now load the font
    let mut path = PathBuf::from(SDCARD_ROOT);
    path.push(format!(
        ".system/res/{}",
        FONTS[get_font_preference().unwrap_or(0)]
    ));
    println!("Loading font: {}", path.display());
    let mut font_bytes = vec![];
    std::fs::File::open(path)?.read_to_end(&mut font_bytes)?;

    let mut font_data: BTreeMap<String, Arc<FontData>> = BTreeMap::new();

    let mut families = BTreeMap::new();

    font_data.insert(
        "custom_font".to_owned(),
        std::sync::Arc::new(FontData::from_owned(font_bytes)),
    );

    families.insert(FontFamily::Proportional, vec!["custom_font".to_owned()]);
    families.insert(FontFamily::Monospace, vec!["custom_font".to_owned()]);

    Ok(FontDefinitions {
        font_data,
        families,
    })
}

fn hint_wrap_nextui_tag(app_state: &'static AppStateManager, tag_name: &str) -> String {
    let mut selected_tag = format!("NextUI {tag_name}");
    if !app_state.release_selection_menu() {
        return selected_tag;
    }
    if !is_most_left_index(app_state) {
        selected_tag = format!("<<     {selected_tag}");
    }
    if !is_most_right_index(app_state) {
        selected_tag = format!("{selected_tag}     >>");
    }
    selected_tag
}

fn is_most_left_index(app_state: &'static AppStateManager) -> bool {
    let index = app_state.nextui_releases_and_tags_index().unwrap_or(0);
    let max_index = app_state
        .nextui_releases_and_tags()
        .unwrap_or_default()
        .len();
    index >= max_index - 1
}

fn is_most_right_index(app_state: &'static AppStateManager) -> bool {
    app_state.nextui_releases_and_tags_index() == Some(0)
}

fn handle_version_navigation(app_state: &'static AppStateManager, direction: i32) {
    if app_state.release_selection_menu() && app_state.release_selection_confirmed() {
        let index = app_state.nextui_releases_and_tags_index().unwrap_or(0);
        if direction < 0 && !is_most_left_index(app_state) {
            // Navigate left (older versions)
            app_state.set_nextui_releases_and_tags_index(Some(index + 1));
        } else if direction > 0 && !is_most_right_index(app_state) {
            // Navigate right (newer versions)
            app_state.set_nextui_releases_and_tags_index(Some(index - 1));
        }
    }
}

#[allow(clippy::too_many_lines)]
pub fn run_ui(
    app_state: &'static AppStateManager,
    mock_display_size: Option<(u32, u32)>,
) -> Result<()> {
    // Initialize SDL and create window
    let (_sdl_context, window, mut event_pump, _controller) = init_sdl(mock_display_size)?;

    // Create OpenGL context and egui painter
    let _gl_context = window.gl_create_context()?;
    let shader_ver = ShaderVersion::Adaptive;
    let dpi_scale = DEFAULT_DPI_SCALE * unsafe { DPI_SCALE_FACTOR };
    let (mut painter, mut egui_state) =
        egui_backend::with_sdl2(&window, shader_ver, DpiScaling::Custom(dpi_scale));

    // Create egui context and set style
    let egui_ctx = egui::Context::default();
    egui_ctx.set_style(setup_ui_style());

    // Font stuff
    if let Ok(fonts) = load_font() {
        egui_ctx.set_fonts(fonts);
    }

    let start_time: Instant = Instant::now();

    loop {
        if app_state.should_quit() {
            break;
        }

        egui_state.input.time = Some(start_time.elapsed().as_secs_f64());
        egui_ctx.begin_pass(egui_state.input.take());

        // UI rendering
        egui::CentralPanel::default().show(&egui_ctx, |ui| {
            ui.vertical_centered(|ui| {
                // Check application state
                let update_in_progress = app_state.current_operation().is_some();

                let title_prefix = format!("NextUI Updater {}", env!("CARGO_PKG_VERSION"));
                if app_state.release_selection_menu() {
                    if app_state.release_selection_confirmed() {
                        ui.label(
                            text(title_prefix + " Version Selector")
                                .color(Color32::from_rgb(150, 150, 150)),
                        );
                    } else {
                        ui.label(
                            text(title_prefix + " Version Selector Warning")
                                .color(Color32::from_rgb(150, 150, 150)),
                        );
                    }
                } else {
                    ui.label(
                        text(title_prefix)
                            .color(Color32::from_rgb(150, 150, 150)),
                    );
                }
                ui.add_space(scale(4.0));

                ui.add_enabled_ui(!update_in_progress, |ui| {
                    let submenu = app_state.submenu();
                    let menu = match submenu {
                        Submenu::NextUI => nextui_ui(ui, app_state),
                    };

                    // Focus the first available button for controller navigation
                    ui.memory_mut(|r| {
                        if r.focused().is_none() {
                            r.request_focus(menu.id);
                        }
                    });
                });

                ui.add_space(scale(8.0));

                // Display current operation
                if let Some(operation) = app_state.current_operation() {
                    ui.label(text(operation).color(Color32::from_rgb(150, 150, 150)));
                }

                // Display error if any
                if let Some(error) = app_state.error() {
                    ui.colored_label(Color32::from_rgb(255, 150, 150), text(error));
                }

                // Show progress bar if available
                if let Some(progress) = app_state.progress() {
                    match progress {
                        Progress::Indeterminate => {
                            ui.add_space(scale(4.0));
                            ui.add(Spinner::new().color(Color32::WHITE));
                        }
                        Progress::Determinate(pr) => {
                            let mut progress_bar = ProgressBar::new(pr);
                            // Show percentage only if progress is > 10% to avoid text
                            // escaping the progress bar
                            if pr > 0.1 {
                                progress_bar = progress_bar.show_percentage();
                            }
                            ui.add(progress_bar);
                        }
                    }
                }
            });

            if !app_state.release_selection_menu() && app_state.current_operation().is_none() {
                egui::Area::new(egui::Id::new("version_selector_indicator"))
                    .anchor(egui::Align2::RIGHT_TOP, Vec2::new(scale(-2.0), scale(-2.0)))
                    .interactable(false)
                    .show(ui.ctx(), |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = scale(4.0);

                            // Draw circle background for button
                            let button_size = scale(6.0);
                            let (rect, _response) = ui.allocate_exact_size(
                                Vec2::splat(button_size),
                                egui::Sense::empty(),
                            );
                            ui.painter().circle(
                                rect.center(),
                                button_size / 2.0,
                                Color32::from_rgb(60, 60, 60),
                                egui::Stroke::new(scale(1.0), Color32::from_rgb(100, 100, 100)),
                            );
                            ui.painter().text(
                                rect.center(),
                                egui::Align2::CENTER_CENTER,
                                "X",
                                egui::FontId::proportional(button_size),
                                Color32::from_rgb(180, 180, 180),
                            );

                            ui.label(
                                RichText::new("Select Version")
                                    .size(button_size)
                                    .color(Color32::from_rgb(100, 100, 100)),
                            );
                        });
                    });
            }

            if let Some(hint) = app_state.hint() {
                egui::Area::new(egui::Id::new("hint_area"))
                    .anchor(egui::Align2::CENTER_BOTTOM, Vec2::new(0.0, scale(-6.0)))
                    .interactable(false)
                    .show(ui.ctx(), |ui| {
                        ui.label(text(hint));
                    });
            }

            // HACK: for some reason dynamic text isn't rendered without this
            ui.allocate_ui(
                Vec2::ZERO,
                |ui| {
                    ui.label(
                        RichText::new(
                            "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789~`!@#$%^&*()-=_+[]{};':\",.<>/?",
                        )
                        .color(Color32::TRANSPARENT)
                    );
                    ui.label(
                        RichText::new(
                            "XSelect Version",
                        )
                        .size(scale(6.0))
                        .color(Color32::TRANSPARENT)
                    );
                },
            );
        });

        // End frame and render
        let FullOutput {
            platform_output,
            textures_delta,
            shapes,
            pixels_per_point,
            viewport_output,
        } = egui_ctx.end_pass();

        let repaint_after = viewport_output
            .get(&egui::ViewportId::ROOT)
            .expect("Missing ViewportId::ROOT")
            .repaint_delay;

        // Process output
        egui_state.process_output(&window, &platform_output);

        // Paint and swap buffers
        let paint_jobs = egui_ctx.tessellate(shapes, pixels_per_point);
        painter.paint_jobs(None, textures_delta, paint_jobs);
        window.gl_swap_window();

        let handle_back_button = || {
            if app_state.release_selection_menu() {
                app_state.set_release_selection_menu(false);
            } else {
                app_state.set_should_quit(true);
            }
        };

        // Process events
        let mut process_event = |event| {
            match event {
                Event::Quit { .. } => app_state.set_should_quit(true),
                Event::ControllerButtonDown {
                    timestamp, button, ..
                } => {
                    if let Some(keycode) = controller_to_key(button) {
                        let key_event = Event::KeyDown {
                            keycode: Some(keycode),
                            timestamp,
                            window_id: window.id(),
                            scancode: Some(sdl2::keyboard::Scancode::Down),
                            keymod: sdl2::keyboard::Mod::empty(),
                            repeat: false,
                        };
                        egui_state.process_input(&window, key_event, &mut painter);
                    }
                }
                Event::ControllerButtonUp {
                    timestamp, button, ..
                } => {
                    if button == sdl2::controller::Button::A {
                        // Exit with "B" button
                        handle_back_button();
                    }

                    if app_state.release_selection_menu() {
                        // Handle left/right navigation in selection menu
                        if button == sdl2::controller::Button::DPadLeft {
                            handle_version_navigation(app_state, -1);
                        } else if button == sdl2::controller::Button::DPadRight {
                            handle_version_navigation(app_state, 1);
                        }
                    } else {
                        // Add X button to reach selection menu
                        if button == sdl2::controller::Button::Y {
                            app_state.set_release_selection_menu(true);
                        }
                    }

                    if let Some(keycode) = controller_to_key(button) {
                        let key_event = Event::KeyUp {
                            keycode: Some(keycode),
                            timestamp,
                            window_id: window.id(),
                            scancode: Some(sdl2::keyboard::Scancode::Down),
                            keymod: sdl2::keyboard::Mod::empty(),
                            repeat: false,
                        };

                        egui_state.process_input(&window, key_event, &mut painter);
                    }
                }
                // for easy testing on desktop
                Event::KeyDown { keycode, .. } => match keycode {
                    Some(sdl2::keyboard::Keycode::Escape) => handle_back_button(),
                    Some(sdl2::keyboard::Keycode::X) => app_state.set_release_selection_menu(true),
                    Some(sdl2::keyboard::Keycode::Left) => handle_version_navigation(app_state, -1),
                    Some(sdl2::keyboard::Keycode::Right) => handle_version_navigation(app_state, 1),
                    _ => {}
                },
                _ => {
                    // Process other input events
                    egui_state.process_input(&window, event, &mut painter);
                }
            }
        };

        if repaint_after.is_zero() {
            for event in event_pump.poll_iter() {
                process_event(event);
            }
        } else if let Some(event) = event_pump.wait_event_timeout(50) {
            process_event(event);
        }
    }

    Ok(())
}
