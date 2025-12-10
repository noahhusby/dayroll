mod app;
mod api;
mod pages;

fn main() {
    yew::Renderer::<app::App>::new().render();
}