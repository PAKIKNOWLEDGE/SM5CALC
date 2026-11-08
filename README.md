# SM5CALC

一个基于 **Etterna MinaCalc 核心算法** 的独立 StepMania (`.sm`) 谱面难度与多维能力属性分析工具。

无需安装或启动 Etterna / StepMania 客户端，直接将 `.sm` 谱面文件拖入窗口，即可在毫秒内还原节奏时序，并计算出 0.70x ~ 2.00x 各倍速下的 **8 维 MSD (Mean Strain Difficulty)** 定数评分。

---

## 🌟 功能特性

- **官方同款计算内核**：直接集成 Etterna 官方源码的 `MinaCalc` 计算引擎（当前版本：**v527**），计算结果与游戏内完全一致。
- **高精度 Timing 状态机**：原生还原 StepMania 的离散事件时序引擎，精准处理 `#BPMS`、`#STOPS`、`#DELAYS`、`#WARPS` 与 `#FAKES`，确保复杂变速与停顿下的时间轴毫秒级对齐。
- **全速域 & 8 维属性分析**：
  - 覆盖 **0.70x 至 2.00x**（步长 0.10x）共 14 档音乐速率。
  - 完整呈现 8 大核心能力指标：`综合 (Overall)`、`Stream`、`Jumpstream (JS)`、`Handstream (HS)`、`Stamina`、`JackSpeed`、`Chordjack (CJ)` 与 `Technical (Tech)`。
- **多维可视化仪表盘**：
  - **7 维极坐标雷达图**：直观展示能力罗盘，支持 1.00x 与所选速率双层多边形动态形变对比。
  - **全曲 NPS 难点波形图**：基于 0.5 秒离散桶采样，标示平均密度线与最大瞬时爆发峰值，支持鼠标划过实时查看毫秒级时间与 NPS。
  - **倍速定数爬升曲线**：折线图可视化各项定数随速率上升的增长趋势，支持在曲线上直接点击选择倍速。
  - **Etterna 经典色彩映射**：为 Stream、JS、HS、Stamina、Jack、CJ、Tech 七大属性赋予社区专属主题色。
- **单文件独立便携**：基于 Rust 与静态链接 C++ 运行时构建，无任何外部 DLL 依赖，启动迅速。

---

## 📥 获取与运行

### 方式一：下载发布版
从 [Releases 页面](https://github.com/PAKIKNOWLEDGE/SM5CALC/releases) 下载最新版的 `sm5calc.exe`，双击即可直接运行。

### 方式二：从源码编译
本项目需要与 `etterna` 源码仓库置于同一上级目录下（`build.rs` 会自动抽取并打补丁编译其中的 `MinaCalc` 模块）：

```
workspace/
├── etterna/           # Etterna 官方源码仓库
└── SM5CALC/           # 本仓库
```

**编译先决条件**：
1. [Rust 工具链](https://rustup.rs/) (Cargo)
2. C++ 编译器（Windows 推荐安装 Visual Studio Build Tools，并勾选 C++ 桌面开发）

**执行编译**：
```powershell
# 进入 SM5CALC 目录
cd SM5CALC

# 编译为高性能 Release 二进制
cargo build --release
```
编译生成的独立单文件位于 `target/release/sm5calc.exe`。

---

## 📊 MinaCalc 核心指标说明

- **计算基准标准**：MinaCalc 采用 **Wife3 评分模型**，以 **93.0%** 得分率作为基准技能水平（Player Skill Needed）进行二分收敛估算。
- **音符过滤规则**：仅常规单键（Tap `1`）、长条头（Hold Head `2`）和滚轮头（Roll Head `4`）参与物理打分；地雷（Mine `M`）、长条尾（Tail `3`）以及落在 Warp / Fake 时间范围内的伪音符会被自动剔除。
- **超密度保护**：若谱面任意 0.5 秒区间内音符行数超过 50 行（即局部瞬时密度 ≥ 100 NPS），MinaCalc 会判定该档位为非人类极限的异常谱面，该倍速档输出将被置为 0。

---

## 📌 当前限制与说明

- **关于 `.ssc` 格式**：目前主要面向 `.sm` 格式解析。部分现代谱包采用包含独立分段变速（Split Timing）的 `.ssc` 格式，该格式暂未支持。
- **键位支持**：完美支持 4K (`dance-single`)、5K、6K、7K 专项模型；其他非常规键位将通过通用模型与键位比率折算。
- **制谱标注定数**：`.sm` 内部的 `#METER` 为制谱者自定义标签，软件中仅展示作对照参考，不参与实际算法计算。

---

## 📄 开源许可与致谢

- 本项目外层界面与解析模块采用 [MIT License](LICENSE)。
- 核心难度计算模块版权属于 [Etterna 团队](https://github.com/etternagame/etterna) (MIT License)。
- 源码不内置 MinaCalc 源代码，编译时自动从同级目录的 `etterna/src/Etterna/MinaCalc` 引入。
