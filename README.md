# RouteFinder

基于 Rust 实现的航路查询工具，使用Fenix导航数据库。输入起飞机场和目的机场的 ICAO 码，自动生成最优航路并展示沿线详细信息。

如果需要API版本，则请查看api分支

## 功能

- **A\* 最短路径搜索** — 基于全球导航数据库构建有向图，使用 A\* 算法（弦长启发式）计算最优航路
- **机场信息查询** — 展示跑道、ILS 进近、ATC 通信频率等完整机场数据
- **航路详情** — 输出航段表（起止点、航路/空域、每段距离）、航路点列表及总里程
- **进离场程序推荐** — 根据航路首末点自动匹配建议的 SID/STAR，同时列出机场全部可用程序

## 依赖

- **Rust** 1.85+ (edition 2024)
- **SQLite** — 通过 `rusqlite` bundled 模式编译，无需额外安装

## 编译

```bash
cd routefinder-rs
cargo build --release
```

编译产物位于 `target/release/routefinder-rs.exe`。

## 使用

### 目录结构

程序启动时查找 `./Navdata/nd.db3` 数据库文件，因此需要将编译好的 exe 放在与 `Navdata` 目录平级的位置：

```
.
├── routefinder-rs.exe
└── Navdata/
    └── nd.db3
    └── cycle.json
    └── cycle_info.txt
```

### 运行

双击 exe 或在终端中直接运行：

```bash
./routefinder-rs
```

启动后显示导航数据库版本及加载耗时，随后进入交互式查询：

```
出发机场: ZBAA
到达机场: ZGGG
```

输入起降机场 ICAO 码即可得到完整航路信息。输入 `q` 退出。

### 输出内容

每次查询输出以下信息：

1. **机场信息** — 位置、标高、过渡高度/高度层、速度限制；跑道列表（编号、航向、尺寸、表面、ILS 详情）；ATC 通信频率（ATIS/放行/地面/塔台/进近/离场/区调/FSS/Unicom）
2. **航线** — 格式如 `ZBAA SID LADIX W40 BTO STAR ZGGG`，附带逐段距离表和总里程
3. **航路点列表** — 序号、Ident、名称、经纬度
4. **建议离场/进场程序** — 匹配当前航路首末点的 SID 和 STAR
5. **全部进离场程序** — 机场所有可用程序及其导航台、跑道尺寸、ILS 参数

## 技术概要

| 模块 | 说明 |
|------|------|
| `graph` | 图结构与 A* 搜索，使用弦长距离作为启发函数（严格下界于大圆距离） |
| `finder` | 航路查找编排：构建航线图、按方位过滤终端程序边、组装结果 |
| `database` | SQLite 数据访问，含 BCD32 频率解码和经纬度格式化 |
| `models` | 共享数据类型（航段、航路点、机场、程序、结果等） |

- 边权重使用 Haversine 大圆距离（海里）
- SID/STAR 边按目标方位过滤（偏差 ≤ 90°，范围 ≤ 400 NM），减少无效搜索
- 启动时建立 11 个数据库索引加速查询
- 机场程序按需懒加载，缓存已初始化机场

## 导航数据库

使用 Fenix 格式的导航数据库（SQLite3）。数据库文件需命名为 `nd.db3` 并放置在可执行文件同级的 `Navdata` 目录下。

数据库包含以下核心表：

| 表 | 内容 |
|----|------|
| Airports | 机场基础信息 |
| Runways | 跑道数据 |
| ILSes | ILS 进近设备 |
| AirportCommunication | ATC 通信频率 |
| Waypoints | 全球航路点 |
| Airways / AirwayLegs | 航路及航段 |
| Terminals / TerminalLegs | SID/STAR 终端程序 |
| Navaids / NavaidTypes | 导航台及类型 |
| config | 数据库版本信息 |

## License

MIT
