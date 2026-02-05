# PrefixDDNS

PrefixDDNS 是一个基于 Rust 的工具，旨在监听 Linux 接口上的 IPv6 前缀变化，并自动触发自定义 Webhook。它特别适用于具有动态 IPv6 前缀的环境（例如家庭宽带 ISP），允许您在前缀发生变化时更新 DNS 记录（如 Cloudflare）或防火墙规则。

## 核心运行逻辑

PrefixDDNS 的核心工作流程如下：

1.  **监听 (Monitor)**:
    - 程序启动后，会通过 Netlink 协议（Linux 内核通信机制）监听网络接口的地址变化事件。
    - 它专门过滤 IPv6 地址消息，忽略 Loopback、Multicast 和 Link-Local (`fe80::/10`) 地址。
    - **启动时检测**: 如果配置了 `run_on_startup: true`，程序启动时会立即扫描当前已有的 IPv6 全局地址并触发任务。

2.  **处理 (Process)**:
    - 当检测到一个新的有效 IPv6 地址（例如 `2001:db8::1234`）时，程序会遍历 `config.json` 中配置的所有任务 (`tasks`)。
    - **地址组合**: 对于每个任务，程序提取检测到的 IPv6 地址的前 64 位（Prefix），并将其与任务配置中定义的后缀 (`suffix`) 组合，生成完整的目标 IPv6 地址。
      - 例如：检测到 `2001:db8:1:1::abc`，任务后缀为 `::1`。
      - 组合结果：`2001:db8:1:1::1`。

3.  **触发 (Trigger)**:
    - 程序使用组合后的 IP 地址，按照任务配置的 Webhook URL、Method、Headers 和 Body 发送 HTTP 请求。
    - 支持在 URL 和 Body 中使用变量替换（见下文）。

## 前后端交互逻辑 (Frontend-Backend Interaction)

PrefixDDNS 内置了一个 Web 服务器（基于 Axum），用于提供仪表盘界面和 API 接口。

- **默认端口**: `3000`
- **前端文件目录**: `static/` (编译时嵌入二进制，开发时从磁盘读取)

如果你希望开发自己的前端，请参考以下协议。

### 1. 静态资源 (Static Assets)

后端会将 `static/` 目录下的文件映射到根路径：

- `http://localhost:3000/` -> `static/index.html`
- `http://localhost:3000/css/style.css` -> `static/css/style.css`
- `http://localhost:3000/js/app.js` -> `static/js/app.js`
- `http://localhost:3000/logo.svg` -> `static/logo.svg`

### 2. API 接口 (API Endpoints)

所有 API 均返回 JSON 格式数据。

#### 获取配置 (Get Configuration)

- **URL**: `GET /api/config`
- **描述**: 获取当前应用的完整配置。
- **响应示例**:
  ```json
  {
    "log_limit": 100,
    "run_on_startup": false,
    "tasks": [
      {
        "id": "task-uuid",
        "name": "Cloudflare NAS",
        "suffix": "::1",
        "webhook_url": "https://api.cloudflare.com/...",
        "webhook_method": "PUT",
        "webhook_headers": {
          "Authorization": "Bearer token",
          "Content-Type": "application/json"
        },
        "webhook_body": "{\"content\": \"{{combined_ip}}\", ...}",
        "enabled": true,
        "allow_api_trigger": true
      }
    ]
  }
  ```

#### 更新配置 (Update Configuration)

- **URL**: `POST /api/config`
- **描述**: 更新配置并保存到磁盘 (`config.json`)。
- **请求体**: 发送完整的配置对象（同 GET 响应结构）。

#### 测试 Webhook (Test Webhook)

- **URL**: `POST /api/test-webhook`
- **描述**: 不保存配置，直接使用请求中的参数进行一次 Webhook 发送测试。
- **请求体**:
  ```json
  {
    "task": { ...Task对象... },
    "fake_ip": "2001:db8::1" // 模拟检测到的 IP
  }
  ```
- **响应**: 纯文本字符串，指示成功或失败信息。

#### 手动触发任务 (Manual Trigger)

- **URL**: `POST /api/trigger/:task_name`
- **描述**: 手动触发已存在的任务。任务必须开启 `allow_api_trigger`。
- **请求体**:
  ```json
  {
    "ip": "2001:db8::1" // 使用此 IP 进行组合和触发
  }
  ```
- **响应**:
  ```json
  {
    "status": "success", // 或 "error"
    "message": "Webhook triggered",
    "data": null
  }
  ```

### 3. 实时日志 (Real-time Logs)

前端通过 Server-Sent Events (SSE) 接收实时运行日志。

- **URL**: `GET /events`
- **协议**: EventSource
- **数据格式**: 每条消息的 `data` 字段为一个 JSON 对象。
- **逻辑**: 连接建立时，服务器会首先发送最近的 `log_limit` 条历史日志（顺序可能为倒序，前端需处理），随后推送实时产生的新日志。

**日志对象结构**:

```json
{
  "timestamp": "2023-10-27 10:00:00",
  "level": "info", // "info" | "success" | "error" | "debug"
  "message": "Detected new IPv6 prefix from: 2001:db8::1"
}
```

## 变量替换

在 Webhook URL 和 Body 中可以使用以下变量：

- `{{combined_ip}}`: 组合后的完整 IPv6 地址（前缀 + 后缀）。
- `{{prefix}}`: 检测到的前缀（格式如 `2001:db8::/64`）。
- `{{original_ip}}`: 接口上检测到的原始 IPv6 地址。
- `{{input_ip}}`: (仅手动触发时) 输入的 IP 地址。

## 安装与运行

### Docker (推荐)

项目提供了 `Dockerfile`，可以直接构建 Docker 镜像运行。

1.  **构建镜像**:

    ```bash
    # 需要先编译二进制文件 (参考下文 Make 命令)
    make x86_64 # 或 make aarch64
    docker build -t prefixddns .
    ```

2.  **运行容器**:

    ```bash
    docker run -d \
      --name prefixddns \
      --network host \
      -v $(pwd)/data:/data \
      prefixddns
    ```

    - 建议使用 `--network host` 以便容器能准确监听宿主机的 Netlink 事件。
    - 挂载 `/data` 目录以持久化保存 `config.json`。

### 手动编译运行

1.  **交叉编译**:
    项目提供了 `Makefile` 用于方便地进行交叉编译（主要针对 musl libc）。
    - **x86_64-unknown-linux-musl**:
      ```bash
      make x86_64
      ```
    - **aarch64-unknown-linux-musl**:
      ```bash
      make aarch64
      ```

    > **注意**: 编译需要安装对应的交叉编译器 (如 `x86_64-linux-gcc` 或 `aarch64-linux-gcc`)。

2.  **常规构建**:

    ```bash
    cargo build --release
    ```

3.  **运行**:

    ```bash
    ./target/release/prefixddns
    ```

    **命令行参数**:
    - `-d, --work-dir <PATH>`: 设置工作目录。
    - `-c, --config <FILE>`: 指定配置文件路径（默认为 `config.json`）。
    - `-p, --port <PORT>`: 指定 Web 服务器端口（默认为 `3000`）。
    - `-i, --interface <NAME>`: 指定要监听的网络接口（如 `eth0`）。如果不指定，则监听所有接口。

4.  **访问**:
    打开浏览器访问 `http://localhost:3000`。

## 目录结构

- `src/`: Rust 源代码
  - `main.rs`: 主程序入口
  - `netlink.rs`: 网络监听模块
  - `web.rs`: Web 服务器与 API 实现
  - `config.rs`: 配置管理
  - `logging.rs`: 日志处理模块
- `static/`: 前端静态资源 (HTML/CSS/JS)
- `config.json`: 配置文件 (运行时生成)
- `Makefile`: 交叉编译脚本
