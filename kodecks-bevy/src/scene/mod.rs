use bevy::prelude::*;
use bevy_mod_picking::DefaultPickingPlugins;

pub mod app;
pub mod config;
pub mod game;
pub mod menu;
pub mod translator;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default, States)]
pub enum GlobalState {
    #[default]
    AppInit,
    MenuMain,
    GameInit,
    GameLoading,
    GameMain,
    GameResult,
    GameCleanup,
}

pub struct ScenePlugin;

impl Plugin for ScenePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(DefaultPickingPlugins)
            .add_plugins(app::AppLoadingPlugin)
            .add_plugins(menu::MenuPlugin)
            .add_plugins(game::loading::GameLoadingPlugin)
            .add_plugins(game::main::GameMainPlugin)
            .add_plugins(game::result::GameResultPlugin)
            .add_plugins(game::cleanup::GameCleanupPlugin)
            .add_plugins(game::camera::CameraPlugin);
    }
}