import 'dart:async';
import 'dart:math';

import 'package:audioplayers/audioplayers.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

void main() => runApp(const CarapaceApp());

/// The carapace "medieval door" skin renders into a Flutter external [Texture] with a TRANSPARENT
/// arched opening; a live, REAL Flutter music player (audioplayers) is framed by the doorway.
/// Tapping the stone wall torch round-trips Dart→Swift→engine→host and re-renders the skin.
class CarapaceApp extends StatefulWidget {
  const CarapaceApp({super.key});
  @override
  State<CarapaceApp> createState() => _CarapaceAppState();
}

class _CarapaceAppState extends State<CarapaceApp> with SingleTickerProviderStateMixin {
  static const _ch = MethodChannel('carapace');
  static const _cw = 360.0, _chh = 640.0; // skin design canvas
  int? _texId;

  // real audio
  final AudioPlayer _audio = AudioPlayer();
  late final AnimationController _spin; // album rotation, runs while playing
  bool _playing = false;
  Duration _pos = Duration.zero;
  Duration _dur = Duration.zero;

  // torch flame driver — pushes the skin's "lit" value in time with the music
  Timer? _flameTimer;
  final Random _rng = Random();
  double _flicker = 0.5;
  Duration _basePos = Duration.zero;
  final Stopwatch _sw = Stopwatch(); // interpolates position between updates
  static const _bpm = 112.0; // Ameno Amapiano groove

  @override
  void initState() {
    super.initState();
    _spin = AnimationController(vsync: this, duration: const Duration(seconds: 12));
    _setUpAudio();
    _flameTimer = Timer.periodic(const Duration(milliseconds: 80), (_) => _driveFlame());
    _init();
  }

  Future<void> _setUpAudio() async {
    await _audio.setReleaseMode(ReleaseMode.loop);
    _audio.onDurationChanged.listen((d) => mounted ? setState(() => _dur = d) : null);
    _audio.onPositionChanged.listen((p) {
      _basePos = p;
      _sw
        ..reset()
        ..start();
      if (mounted) setState(() => _pos = p);
    });
    _audio.onPlayerStateChanged.listen((s) {
      if (!mounted) return;
      final playing = s == PlayerState.playing;
      setState(() => _playing = playing);
      if (playing) {
        _spin.repeat();
      } else {
        _spin.stop();
      }
    });
    try {
      await _audio.setSource(AssetSource('ameno.mp3')); // loads duration; paused
    } catch (_) {}
  }

  @override
  void dispose() {
    _flameTimer?.cancel();
    _spin.dispose();
    _audio.dispose();
    super.dispose();
  }

  // Push the torch flame level to the skin ~12x/s: a beat pulse (synced to the real playback
  // position + tempo) plus a candle flicker while playing; a low steady ember when paused.
  void _driveFlame() {
    if (_texId == null) return;
    double lit;
    if (_playing) {
      final t = _basePos.inMilliseconds / 1000.0 + _sw.elapsedMilliseconds / 1000.0;
      final beat = t * _bpm / 60.0;
      final phase = beat - beat.floorToDouble(); // 0..1 within the beat
      final pulse = exp(-phase * 5.0); // bright on the beat, decays
      _flicker = (_flicker + (_rng.nextDouble() - 0.5) * 0.5).clamp(0.0, 1.0);
      lit = (0.30 + 0.55 * pulse + 0.16 * _flicker).clamp(0.15, 1.0);
    } else {
      lit = 0.28; // low steady ember
    }
    _ch.invokeMethod('setLit', {'v': lit});
  }

  // Carapace is set up a moment AFTER Flutter startup (deferred to dodge a VSyncClient race).
  Future<void> _init() async {
    for (var i = 0; i < 100; i++) {
      try {
        final id = await _ch.invokeMethod<int>('textureId');
        if (id != null && id >= 0) {
          if (mounted) setState(() => _texId = id);
          return;
        }
      } on MissingPluginException {
        // channel not ready
      } on PlatformException {
        // transient
      }
      await Future.delayed(const Duration(milliseconds: 80));
    }
  }

  Future<void> _tapSkin(Offset local, Size size) async {
    await _ch.invokeMethod('tap', {
      'x': local.dx / size.width * _cw,
      'y': local.dy / size.height * _chh,
    });
  }

  Future<void> _togglePlay() async {
    if (_playing) {
      await _audio.pause();
    } else {
      await _audio.resume();
    }
  }

  double get _progress =>
      _dur.inMilliseconds == 0 ? 0.0 : (_pos.inMilliseconds / _dur.inMilliseconds).clamp(0.0, 1.0);

  Future<void> _seekFrac(double f) async {
    if (_dur.inMilliseconds == 0) return;
    await _audio.seek(_dur * f.clamp(0.0, 1.0));
  }

  @override
  Widget build(BuildContext context) {
    final scr = MediaQuery.of(context).size;
    final fw = min(scr.width - 20, 400.0);
    final fh = fw * _chh / _cw;
    final ox = 90 / _cw * fw, oy = 250 / _chh * fh;
    final ow = 180 / _cw * fw, oh = 315 / _chh * fh;

    return MaterialApp(
      debugShowCheckedModeBanner: false,
      home: Scaffold(
        backgroundColor: const Color(0xFF0B0A08),
        body: Center(
          child: _texId == null
              ? const CircularProgressIndicator(color: Color(0xFFE2964A))
              : SizedBox(
                  width: fw,
                  height: fh,
                  child: Stack(
                    children: [
                      Positioned.fill(child: Texture(textureId: _texId!)),
                      Positioned.fill(
                        child: GestureDetector(
                          behavior: HitTestBehavior.translucent,
                          onTapDown: (d) => _tapSkin(d.localPosition, Size(fw, fh)),
                        ),
                      ),
                      Positioned(left: ox, top: oy, width: ow, height: oh, child: _player(ow, oh)),
                    ],
                  ),
                ),
        ),
      ),
    );
  }

  static const _gold = Color(0xFFE2964A);
  static const _goldHot = Color(0xFFF3C57A);

  Widget _player(double w, double h) {
    final art = w * 0.56;
    return Padding(
      padding: EdgeInsets.only(top: h * 0.14, bottom: 22, left: 8, right: 8),
      child: Column(
        mainAxisAlignment: MainAxisAlignment.spaceBetween,
        children: [
          AnimatedBuilder(
            animation: _spin,
            builder: (_, __) => Transform.rotate(angle: _spin.value * 2 * pi, child: _albumArt(art)),
          ),
          const Column(
            children: [
              Text('Ameno Amapiano',
                  style: TextStyle(color: Colors.white, fontSize: 20, fontWeight: FontWeight.w600, letterSpacing: 0.2)),
              SizedBox(height: 3),
              Text('Goya Menor & Nektunez',
                  style: TextStyle(color: Colors.white54, fontSize: 13, letterSpacing: 0.3)),
            ],
          ),
          _scrubber(w),
          _transport(),
        ],
      ),
    );
  }

  Widget _albumArt(double d) => Container(
        width: d,
        height: d,
        decoration: BoxDecoration(
          shape: BoxShape.circle,
          gradient: const RadialGradient(
            center: Alignment(-0.4, -0.4),
            radius: 0.95,
            colors: [Color(0xFF3B2E22), Color(0xFF120E0A)],
          ),
          boxShadow: [BoxShadow(color: _gold.withValues(alpha: 0.25), blurRadius: 22, spreadRadius: 1)],
        ),
        child: Stack(
          alignment: Alignment.center,
          children: [
            Container(
              width: d * 0.62,
              height: d * 0.62,
              decoration: BoxDecoration(
                shape: BoxShape.circle,
                border: Border.all(color: _gold.withValues(alpha: 0.6), width: 1.5),
              ),
            ),
            Icon(Icons.local_fire_department, color: _goldHot.withValues(alpha: 0.9), size: d * 0.34),
            Container(
              width: d * 0.10,
              height: d * 0.10,
              decoration: const BoxDecoration(shape: BoxShape.circle, color: Color(0xFF0B0A08)),
            ),
          ],
        ),
      );

  Widget _scrubber(double w) {
    final pos = _progress;
    return Column(
      children: [
        GestureDetector(
          behavior: HitTestBehavior.opaque,
          onTapDown: (d) => _seekFrac(d.localPosition.dx / (w - 16)),
          onHorizontalDragUpdate: (d) => _seekFrac(pos + d.primaryDelta! / (w - 16)),
          child: SizedBox(
            height: 22,
            width: w - 16,
            child: Stack(
              alignment: Alignment.centerLeft,
              children: [
                Container(height: 3, decoration: BoxDecoration(color: Colors.white24, borderRadius: BorderRadius.circular(2))),
                FractionallySizedBox(
                  widthFactor: pos,
                  child: Container(height: 3, decoration: BoxDecoration(color: _gold, borderRadius: BorderRadius.circular(2))),
                ),
                Align(
                  alignment: Alignment(pos * 2 - 1, 0),
                  child: Container(
                    width: 11, height: 11,
                    decoration: BoxDecoration(shape: BoxShape.circle, color: _goldHot, boxShadow: [BoxShadow(color: _gold.withValues(alpha: 0.7), blurRadius: 6)]),
                  ),
                ),
              ],
            ),
          ),
        ),
        const SizedBox(height: 4),
        Padding(
          padding: const EdgeInsets.symmetric(horizontal: 2),
          child: Row(
            mainAxisAlignment: MainAxisAlignment.spaceBetween,
            children: [
              Text(_fmt(_pos), style: const TextStyle(color: Colors.white38, fontSize: 11, fontFeatures: [FontFeature.tabularFigures()])),
              Text(_fmt(_dur), style: const TextStyle(color: Colors.white38, fontSize: 11, fontFeatures: [FontFeature.tabularFigures()])),
            ],
          ),
        ),
      ],
    );
  }

  Widget _transport() => Row(
        mainAxisAlignment: MainAxisAlignment.center,
        children: [
          IconButton(
            onPressed: () => _seekFrac(_progress - 0.1),
            icon: const Icon(Icons.replay_10_rounded, color: Colors.white70, size: 28),
          ),
          const SizedBox(width: 6),
          GestureDetector(
            onTap: _togglePlay,
            child: Container(
              width: 58, height: 58,
              decoration: const BoxDecoration(shape: BoxShape.circle, gradient: LinearGradient(colors: [_goldHot, _gold], begin: Alignment.topLeft, end: Alignment.bottomRight)),
              child: Icon(_playing ? Icons.pause_rounded : Icons.play_arrow_rounded, color: const Color(0xFF1A120A), size: 34),
            ),
          ),
          const SizedBox(width: 6),
          IconButton(
            onPressed: () => _seekFrac(_progress + 0.1),
            icon: const Icon(Icons.forward_10_rounded, color: Colors.white70, size: 28),
          ),
        ],
      );

  String _fmt(Duration d) {
    final m = d.inMinutes;
    final s = d.inSeconds % 60;
    return '$m:${s.toString().padLeft(2, '0')}';
  }
}
