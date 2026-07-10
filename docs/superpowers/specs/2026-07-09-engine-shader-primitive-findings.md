# Engine `shader{}` primitive — Task 1 spike findings (GO/NO-GO)

**Verdict: GO.** The 4-stage compositing order works exactly as designed. A pre-filled "shader
background" target, plus vello 2D rendered into a *transparent* offscreen, plus a premultiplied-
alpha composite-over, yields "2D over shader background" against real GPU pixels: the transparent
region of the vello layer reveals the blue background (`[0,0,255]`), and the opaque red 2D lands
as opaque red over it (`[255,0,0]`). Test: `four_stage_composites_2d_over_shader_background`
(`crates/carapace/tests/shader_spike.rs`), passing.

**Measured added per-frame cost (64×64, debug build, this Mac):** pipelined ~2.5–7 ms/frame
(submit N frames, poll once — the realistic 60fps-loop number); a blocking poll-per-frame variant
runs ~4–21 ms (pessimistic — it forces a CPU/GPU sync bubble every frame, no cross-frame
pipelining, and is noisy under debug + shared machine). Both are debug-build upper bounds; release
+ pipelining puts this comfortably under a 16.6 ms 60fps budget. The existing view-compositor and
paper-shader work already free-run at 60fps with the same composite pass, corroborating the GO.

**Surprises:** none blocking. (1) vello's transparent-offscreen alpha behaves as expected —
`base_color` alpha 0 leaves undrawn pixels premultiplied-transparent, and
`PREMULTIPLIED_ALPHA_BLENDING` composites them correctly with no purple/dark fringing in the
"red" region (vello outputs premultiplied RGBA, matching the blend mode). (2) The only wgpu gotcha
was texture usage flags: the vello target offscreen must carry `TEXTURE_BINDING` to be sampled as
the composite source, and (per this repo's readback rig) `STORAGE_BINDING`; a target texture can't
cross wgpu devices, so the second offscreen is created on the same device as the first.
