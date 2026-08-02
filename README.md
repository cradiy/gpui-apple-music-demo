# GPUI Apple Music Demo

一个有点像 Apple Music、但主要任务是负责“哇，GPUI 还能这样？”的播放器界面 demo。

An Apple Music-ish player UI demo whose real job is to make people say, “Wait, GPUI can do that?”

![Fullscreen now playing view](screenshots/2.png)

![Library and lyrics view](screenshots/1.png)

## 中文

这是一个用我 fork 的 [GPUI](https://github.com/cradiy/gpui) 写的 Apple Music 风格 demo。它能真的播放音乐、读元数据和歌词，也有一些挺唬人的动效；不过请先放下那份 20 万首歌的曲库——它不是一个完整的播放器，也暂时没有报名参加 Spotify 收购计划。

我写它主要是为了展示 GPUI 在桌面 UI、媒体播放和动态视觉效果上的能力，同时验证我在 fork 中增加和改进的功能，可以在一个真实一点、好看一点的场景里一起工作。

### 现在能演示什么

- Apple Music 风格的窗口布局、玻璃侧边栏和播放控制条
- 带封面取色与动态光晕的全屏 Now Playing 界面
- 基于 `gpui_media` 的真实音频播放、进度与控制
- 内嵌元数据、封面和歌词解析
- 普通逐行 LRC、带结束时间的 LRC，以及逐字歌词 UI
- 歌词高亮、平滑滚动、长音节动效与间奏省略号
- Linux MPRIS 媒体状态，可被桌面面板和媒体键识别
- 可输入的搜索框——目前主要负责证明它确实能输入

### 运行

目前这个 demo 面向 Linux（Wayland/X11），需要可用的 Rust 工具链，以及 GPUI / `gpui_media` 所需的系统环境。

```bash
cargo run --release -- "/path/to/your/song.mp3"
```

如果音频旁边有同名 `.lrc`，程序会自动读取：

```text
若山詩音 - ツバサ.mp3
若山詩音 - ツバサ.lrc
```

没有同名歌词也没关系，音乐照播；右边只会安静一点。程序也会尝试读取音频文件中的内嵌歌词、封面、标题和歌手信息。

### 它不是什么

它没有完整的曲库管理、播放队列、歌单同步、账号系统、云端服务或“猜你喜欢但其实猜错了”。很多列表和入口仍然是界面演示内容。这个仓库更像 GPUI 的一张动态名片，而不是你应该立刻迁移全部音乐数据的地方。

## English

This is an Apple Music-inspired demo built with [my GPUI fork](https://github.com/cradiy/gpui). It really plays audio, reads metadata and lyrics, and comes with enough motion and glow to look suspiciously confident. Still, please put down that 200,000-track library: this is not a complete music player, and Spotify has not called yet.

I built it to showcase what GPUI can do for desktop UI, media playback, and animated visual effects—and to exercise the features and improvements in my fork inside something more convincing than a rectangle named `example`.

### What it demonstrates

- An Apple Music-like window, glass sidebar, and compact player bar
- A fullscreen Now Playing view with artwork-driven animated glow
- Real audio playback, progress, and controls powered by `gpui_media`
- Embedded metadata, artwork, and lyric extraction
- Line-timed LRC, LRC with explicit end times, and word-timed lyric UI
- Lyric highlighting, smooth scrolling, long-syllable motion, and animated gap dots
- Linux MPRIS integration for desktop panels and media keys
- A working search input—search results are taking a more relaxed approach

### Run it

The demo currently targets Linux (Wayland/X11). You will need a Rust toolchain and the system environment required by GPUI and `gpui_media`.

```bash
cargo run --release -- "/path/to/your/song.mp3"
```

Place a same-name `.lrc` file beside the audio file and it will be picked up automatically:

```text
Artist - Song.mp3
Artist - Song.lrc
```

No sidecar lyrics? The song still plays; the right side simply becomes a little more contemplative. Embedded lyrics, artwork, title, and artist metadata are also read when available.

### What it is not

There is no complete library manager, persistent queue, playlist sync, account system, cloud service, or eerily accurate recommendation engine. Several library entries and controls are presentation material. Think of this repository as a moving GPUI showcase, not the place to entrust your only copy of a music collection.

## License and credits

本 demo 采用 [MIT License](LICENSE)，喜欢就拿去玩。要是你顺手把它做成了完整播放器，记得回来告诉我。

This demo is available under the [MIT License](LICENSE). Fork it, tweak it, make it glow harder—and if it somehow turns into a complete music player, do come back and tell me.
