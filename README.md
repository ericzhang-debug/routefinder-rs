# RouteFinder API

基于 Actix-web 的航路查询 REST API 服务，使用 Fenix 导航数据库。提供航路规划、机场信息、进离场程序、航点/航路查询等接口。

## 功能

- **航路查询** — 输入起降机场 ICAO 码，返回 A* 最优航路（航路文本、航段距离、航路点列表）
- **机场信息** — 跑道、ILS 进近、ATC 通信频率等完整机场数据
- **程序查询** — 查询 SID/STAR 进离场程序，支持按航路点推荐匹配
- **航点 / 航路** — 航点经纬度与导航台信息查询，航路途径航路点列表
- **API 首页** — 浏览器访问 `/` 即可查看完整的交互式接口文档
- **JSON 响应** — 所有接口返回结构化 JSON，便于前端或第三方集成

## API 端点

### 航路查询

| 端点 | 说明 |
|------|------|
| `GET /route?dep=ZBAA&arr=ZGGG` | 完整航路查询（含机场、航段、航路点、程序） |
| `GET /route/text?dep=ZBAA&arr=ZGGG` | 仅返回航路文本字符串 |
| `GET /route/legs?dep=ZBAA&arr=ZGGG` | 仅返回航段列表 |
| `GET /route/waypoints?dep=ZBAA&arr=ZGGG` | 仅返回航路点列表 |

### 机场信息

| 端点 | 说明 |
|------|------|
| `GET /airport/{icao}` | 机场完整信息（跑道、ILS、通讯频率） |
| `GET /airport/{icao}/runways` | 仅返回跑道列表 |
| `GET /airport/{icao}/communications` | 仅返回通讯频率 |

### 程序

| 端点 | 说明 |
|------|------|
| `GET /airport/{icao}/procedures?type=SID` | 机场所有进离场程序（type 可选 SID/STAR） |
| `GET /airport/{icao}/procedures/suggested?wpt=DUGEB&type=SID` | 根据航路点推荐匹配的程序 |
| `GET /airport/{icao}/departures` | 机场所有离场程序 (SID) |
| `GET /airport/{icao}/arrivals` | 机场所有进场程序 (STAR) |

### 航点 / 航路 / 元数据

| 端点 | 说明 |
|------|------|
| `GET /waypoint/{ident}` | 航点信息（经纬度、名称、导航台类型频率） |
| `GET /airway/{ident}` | 航路所有航路点（按顺序含经纬度） |
| `GET /navdata` | 导航数据库版本信息 |
| `GET /health` | 健康检查 |
| `GET /` | API 文档首页 (HTML) |

## 依赖

- **Rust** 1.85+
- **Actix-web** 4 — HTTP 服务框架
- **SQLite** — 通过 `rusqlite` bundled 模式编译，无需额外安装

## 编译

```bash
cd routefinder-api
cargo build --release
```

编译产物位于 `target/release/routefinder-api.exe`。

## 使用

### 目录结构

程序启动时查找 `./Navdata/nd.db3` 数据库文件：

```
.
├── routefinder-api.exe
└── Navdata/
    └── nd.db3
```

### 运行

```bash
./routefinder-api
```

服务默认监听 `0.0.0.0:3001`，启动输出：

```
[Init] Opening database...
[Init] Creating indices...
[Init] Loading navigation data...
[Init] Ready. Starting server on http://0.0.0.0:3001
```

浏览器打开 `http://localhost:3001` 查看接口文档，或直接调用 API：

```bash
curl "http://localhost:3001/route?dep=ZBAA&arr=ZGGG"
```

### 响应示例

`GET /route?dep=ZBAA&arr=ZGGG` 返回：

```json
{
  "route": "ZBAA SID DUGEB W40 BTO A461 LIG STAR ZGGG",
  "total_distance_nm": 1072.3,
  "legs": [
    {"from_name": "ZBAA", "to_name": "DUGEB", "route": "SID", "distance_nm": 12.5},
    ...
  ],
  "waypoints": [
    {"name": "ZBAA", "latitude": 40.072, "longitude": 116.597},
    ...
  ],
  "departure_info": { "icao": "ZBAA", "name": "北京/首都", ... },
  "arrival_info": { "icao": "ZGGG", "name": "广州/白云", ... },
  "suggested_sids": [...],
  "suggested_stars": [...]
}
```

## 技术概要

| 模块 | 说明 |
|------|------|
| `main` | Actix-web HTTP 服务，路由注册与请求日志 |
| `finder` | 航路查找编排：构建航线图、按方位过滤终端程序边、组装结果 |
| `graph` | 图结构与 A* 搜索，使用弦长距离作为启发函数 |
| `database` | SQLite 数据访问，含 BCD32 频率解码和经纬度格式化 |
| `models` | 共享数据类型与 Serde 序列化 |

- 边权重使用 Haversine 大圆距离（海里）
- SID/STAR 边按目标方位过滤（偏差 ≤ 90°，范围 ≤ 400 NM）
- 启动时建立 11 个数据库索引加速查询
- 机场程序按需懒加载，缓存已初始化机场
- 所有接口为同步处理，Actix 以线程池方式并行响应请求

## 与 CLI 版本的关系

本项目是 [routefinder-rs](../routefinder-rs) 的 API 版本，共享相同的航线引擎核心（`graph`、`finder`、`database`、`models`），不同之处在于：

| | routefinder-rs (CLI) | routefinder-api (本项目) |
|---|---|---|
| 交互方式 | 终端交互式输入 | HTTP REST API |
| 输出格式 | 格式化文本表格 | JSON |
| 适用场景 | 单次查询 / 调试 | 前端集成 / 自动化 |

## 导航数据库

使用 Fenix 格式的导航数据库（SQLite3）。数据库文件需命名为 `nd.db3` 并放置在可执行文件同级的 `Navdata` 目录下。

## License

MIT
