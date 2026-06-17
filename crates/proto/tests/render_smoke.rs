use proto::host::MediaHost;
use proto::render::Renderer;
use proto::scene::{Color, Node, Pt, Scene};

#[test]
fn renders_a_filled_rect_at_expected_pixel() {
    // A red 100x100 rect on a black-ish canvas; center pixel must be red.
    let scene = Scene {
        canvas: (200, 200),
        nodes: vec![Node::Fill {
            path: vec![
                Pt { x: 50.0, y: 50.0 },
                Pt { x: 150.0, y: 50.0 },
                Pt { x: 150.0, y: 150.0 },
                Pt { x: 50.0, y: 150.0 },
            ],
            color: Color { r: 255, g: 0, b: 0 },
        }],
    };
    let mut r = Renderer::new();
    let pm = r.render(&scene, &MediaHost::new());
    let i = ((100 * 200 + 100) * 4) as usize; // pixel (100,100)
    assert_eq!(&pm.data[i..i + 3], &[255, 0, 0], "center should be red");
}
