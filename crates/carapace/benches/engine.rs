use std::time::Duration;

use carapace::command::SkinSource;
use carapace::engine::{Engine, PointerEvent};
use carapace::fixture::FixtureHost;
use carapace::scene::Pt;
use carapace::vocab::VocabRegistry;
use criterion::{Criterion, criterion_group, criterion_main};

const SKIN: &str = r#"
    region{ path={{x=0,y=0},{x=100,y=0},{x=100,y=100},{x=0,y=100}},
            on_press=function() host.toggle() end }
    value_fill{ path={{x=0,y=120},{x=200,y=120},{x=200,y=140},{x=0,y=140}},
                value='level', color={r=1,g=2,b=3} }
"#;

fn src(s: &str) -> SkinSource {
    SkinSource::inline(s, (200, 200))
}

fn engine() -> Engine {
    Engine::new(
        Box::new(FixtureHost::new()),
        VocabRegistry::base(),
        src(SKIN),
    )
    .unwrap()
}

fn benches(c: &mut Criterion) {
    c.bench_function("scene_hit", |b| {
        let e = engine();
        b.iter(|| e.scene().hit(std::hint::black_box(Pt { x: 50.0, y: 50.0 })));
    });
    c.bench_function("drain_toggle", |b| {
        let mut e = engine();
        b.iter(|| {
            e.handle_pointer(Pt { x: 50.0, y: 50.0 }, PointerEvent::Press);
            e.update(Duration::ZERO);
        });
    });
    c.bench_function("scene_rebuild", |b| {
        b.iter(|| std::hint::black_box(engine()));
    });
    c.bench_function("render_frame", |b| {
        // Build a GPU device once; skip if no adapter (e.g. CI without GPU).
        let setup = pollster::block_on(async {
            let instance = wgpu::Instance::default();
            let adapter = instance
                .request_adapter(&wgpu::RequestAdapterOptions::default())
                .await
                .ok()?;
            let (device, queue) = adapter
                .request_device(&wgpu::DeviceDescriptor::default())
                .await
                .ok()?;
            Some((device, queue))
        });
        let Some((device, queue)) = setup else {
            eprintln!("render_frame: no GPU adapter, skipping");
            b.iter(|| ());
            return;
        };
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width: 342,
                height: 394,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut r = carapace::render::Renderer::new(&device);
        let e = engine();
        b.iter(|| {
            r.draw(
                e.scene(),
                |k| e.state(k),
                |_| None,
                &carapace::render::RenderTarget {
                    device: &device,
                    queue: &queue,
                    view: &view,
                    width: 342,
                    height: 394,
                    base_color: carapace::scene::Color {
                        r: 0,
                        g: 0,
                        b: 0,
                        a: 255,
                    },
                    time: e.elapsed_secs(),
                },
            );
        });
    });
}

criterion_group!(g, benches);
criterion_main!(g);
