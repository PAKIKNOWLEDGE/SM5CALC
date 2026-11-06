# SM5CALC

把一首歌的 `.sm` 拖进窗口，看 Etterna 的谱面难度。

大数字是 **1.00 倍综合**。右边是这一倍的七个单项。下面的表是 **0.70 到 2.00**，每 0.10 一档。谱面里手写的定数只作对照，不参与计算。

一首 `.sm` 里往往有好几张谱，窗口上方可以切换。文件夹不会扫描。

## 下载

[Releases](https://github.com/PAKIKNOWLEDGE/SM5CALC/releases) 里的 `sm5calc.exe` 可以直接打开。计算器版本是 **527**，和这份工具编译时用的 Etterna 源码一致。

## 从源码编译

需要 Rust、Visual Studio 的 C++ 生成工具，以及一份 Etterna 源码。`SM5CALC` 和 `etterna` 要放在同一个上级目录：

```
develop/
  etterna/
  SM5CALC/
```

```
cargo build --release
```

程序在 `target/release/sm5calc.exe`。

## 这些数字是什么

综合和单项来自 Etterna 的 MinaCalc。算的是缓存用的谱面难度：目标分数 93%，倍速从 0.70 到 2.00。打完一首之后游戏里按真实得分再算的分数评级，这里不算。

有效音符是 tap，以及长条、滚轮的头。地雷、假键、长条尾巴不进计算。落在 warp 或 fake 区间里的行会丢掉。`#OFFSET` 不会改变难度。

某个半秒里超过 50 行时，那一档会变成 0。

## 读不了的东西

- `.ssc`。很多谱包以 `.ssc` 为准，而且单张谱可以有自己的 BPM。这种歌和游戏里的数字会对不上。
- 一次一首。谱包目录是嵌套的，这里不扫文件夹。
- 4、5、6、7 键有专门的模型。其它键数走通用模型，再按键数缩放。

## 许可

本仓库的界面和 `.sm` 解析是 MIT，见 [LICENSE](LICENSE)。

难度计算使用 Etterna 的 MinaCalc，版权属于 Etterna 贡献者，同样是 MIT：

Copyright (c) 2016-2023 Etterna <etternadev@gmail.com>.

源码不包含 MinaCalc。编译时从旁边的 `etterna/src/Etterna/MinaCalc` 读入。
