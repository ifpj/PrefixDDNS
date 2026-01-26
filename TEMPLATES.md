# PrefixDDNS 任务模板配置指南

PrefixDDNS 支持通过高度可定制的模板来适配各种 DDNS 提供商和 Webhook 服务。本文档详细说明了内置模板的配置方法，以及如何获取所需的关键参数（如 Token、Zone ID、Record ID 等）。

---

## 目录

1.  [变量替换说明](#变量替换说明)
2.  [Cloudflare DNS](#1-cloudflare-dns)
3.  [Dynv6](#2-dynv6)
4.  [Dynu](#3-dynu)
5.  [Afraid.org (FreeDNS)](#4-afraidorg-freedns)
6.  [DuckDNS](#5-duckdns)
7.  [deSEC.io](#6-desecio)
8.  [YDNS](#7-ydns)
9.  [通用 Webhook](#8-通用-webhook)

---

## 变量替换说明

在配置 Webhook 的 **URL** 和 **Body** 时，你可以使用以下变量，程序在运行时会自动将其替换为实际值：

| 变量名 | 说明 | 示例值 |
| :--- | :--- | :--- |
| `{{combined_ip}}` | **最常用**。组合后的完整 IPv6 地址（前缀 + 后缀）。 | `2001:db8::1` |
| `{{prefix}}` | 检测到的 IPv6 前缀。 | `2001:db8::/64` |
| `{{original_ip}}` | 接口上检测到的原始 IPv6 地址。 | `2001:db8::1234` |
| `{{input_ip}}` | 手动触发 API 时输入的 IP 地址（仅手动模式有效）。 | `2001:db8::5678` |

---

## 1. Cloudflare DNS

使用 Cloudflare API v4 更新特定的 DNS 记录。

### 模板配置

- **Method**: `PUT`
- **URL**: `https://api.cloudflare.com/client/v4/zones/YOUR_ZONE_ID/dns_records/YOUR_RECORD_ID`
- **Headers**:
  ```json
  {
    "Authorization": "Bearer YOUR_TOKEN",
    "Content-Type": "application/json"
  }
  ```
- **Body**:
  ```json
  {
    "type": "AAAA",
    "name": "example.com",
    "content": "{{combined_ip}}",
    "ttl": 120,
    "proxied": false
  }
  ```

### 如何获取参数？

#### 1. `YOUR_ZONE_ID` (区域 ID)
1. 登录 Cloudflare Dashboard。
2. 点击进入你的域名（例如 `example.com`）。
3. 在 **Overview** (概览) 页面，向下滑动到右下角。
4. 找到 **API** 区域，复制 **Zone ID**。

#### 2. `YOUR_TOKEN` (API 令牌)
1. 点击右上角头像 -> **My Profile** -> **API Tokens**。
2. 点击 **Create Token**。
3. 使用 **Edit Zone DNS** 模板。
4. 在 **Zone Resources** 中选择 `Include` -> `Specific zone` -> 你的域名。
5. 生成并复制 Token。

#### 3. `YOUR_RECORD_ID` (记录 ID)
记录 ID 无法直接在网页上查看，需要通过 API 获取。你可以使用以下命令（需要先获取 Token 和 Zone ID）：

```bash
# 请替换 YOUR_TOKEN 和 YOUR_ZONE_ID
curl -X GET "https://api.cloudflare.com/client/v4/zones/YOUR_ZONE_ID/dns_records?type=AAAA" \
     -H "Authorization: Bearer YOUR_TOKEN" \
     -H "Content-Type: application/json"
```
在返回的 JSON 结果中，找到对应域名记录的 `"id"` 字段，即为 `YOUR_RECORD_ID`。

---

## 2. Dynv6

Dynv6 提供了两种更新方式：简单 URL 更新（适用于主域名）和 REST API 更新（适用于子域名/特定记录）。

### 模式 A: Dynv6 (Zone) - 推荐用于主域名
- **适用场景**: 更新 `example.dynv6.net` 的主 AAAA 记录。
- **Method**: `GET`
- **URL**:
  ```text
  https://dynv6.com/api/update?hostname=YOUR_HOSTNAME&token=YOUR_TOKEN&ipv6={{combined_ip}}
  ```
- **Body**: `null` (留空)

#### 参数获取:
- **YOUR_HOSTNAME**: 你的完整域名（如 `test.dynv6.net`）。
- **YOUR_TOKEN**: 登录 Dynv6 -> 菜单 **Keys** -> 查看 **HTTP Token**。

### 模式 B: Dynv6 (Subdomain) - 用于特定记录
- **适用场景**: 精确更新区域内的某条记录（如 `sub.example.dynv6.net`），且不影响其他记录。
- **Method**: `PATCH`
- **URL**: `https://dynv6.com/api/v2/zones/YOUR_ZONE_ID/records/YOUR_RECORD_ID`
- **Headers**:
  ```json
  {
    "Authorization": "Bearer YOUR_TOKEN",
    "Content-Type": "application/json"
  }
  ```
- **Body**:
  ```json
  {
    "data": "{{combined_ip}}"
  }
  ```

#### 参数获取:
- **YOUR_TOKEN**: 同上（HTTP Token）。
- **YOUR_ZONE_ID**:
  - 方法1: 登录 Dynv6，点击你的 Zone，浏览器 URL 链接中的数字即为 ID (例如 `https://dynv6.com/zones/123456`，ID 为 `123456`)。
  - 方法2: 使用 API `curl -H "Authorization: Bearer TOKEN" https://dynv6.com/api/v2/zones`。
- **YOUR_RECORD_ID**:
  - 需要通过 API 获取。列出区域内所有记录：
  ```bash
  # 替换 TOKEN 和 ZONE_ID
  curl -H "Authorization: Bearer YOUR_TOKEN" https://dynv6.com/api/v2/zones/YOUR_ZONE_ID/records
  ```
  找到对应 `name` 的记录，复制其 `"id"`。

---

## 3. Dynu

Dynu 支持标准的 IP Update Protocol。

### 模式 A: Dynu (Zone) - 更新主域名
- **Method**: `GET`
- **URL**:
  ```text
  https://api.dynu.com/nic/update?hostname=YOUR_HOSTNAME&myipv6={{combined_ip}}&username=YOUR_USERNAME&password=YOUR_PASSWORD
  ```

### 模式 B: Dynu (Subdomain/Alias) - 更新子域名/别名
- **Method**: `GET`
- **URL**:
  ```text
  https://api.dynu.com/nic/update?hostname=YOUR_ROOT_DOMAIN&alias=YOUR_SUBDOMAIN&myipv6={{combined_ip}}&username=YOUR_USERNAME&password=YOUR_PASSWORD
  ```

### 参数获取与说明

- **YOUR_USERNAME**: 你的 Dynu 登录用户名。
- **YOUR_PASSWORD**: 你的 Dynu 登录密码，**或者**密码的 MD5/SHA256 哈希值（推荐使用哈希值以提高安全性）。
  - 生成 MD5: `echo -n 'your_password' | md5sum`
- **YOUR_HOSTNAME** (模式 A): 你的完整域名 (如 `example.dynu.com`)。
- **YOUR_ROOT_DOMAIN** (模式 B): 根域名 (如 `example.dynu.com`)。
- **YOUR_SUBDOMAIN** (模式 B): 子域名或别名名称 (如 `www` 或 `blog`)。需要在 Dynu 控制面板的 **Aliases** 中预先创建。

---

## 4. Afraid.org (FreeDNS)

Afraid.org 使用基于 Token 的 Direct URL 更新。

- **Method**: `GET`
- **URL**:
  ```text
  https://freedns.afraid.org/dynamic/update.php?YOUR_TOKEN&address={{combined_ip}}
  ```
- **Body**: `null` (留空)

### 如何获取 `YOUR_TOKEN`？

1. 登录 [freedns.afraid.org](https://freedns.afraid.org/)。
2. 进入 **Dynamic DNS** 页面。
3. 在列表底部，找到 **"Direct URL"** 链接。
4. 右键复制该链接。
   - 链接格式通常为：`https://freedns.afraid.org/dynamic/update.php?这里是Token`
   - **注意**：如果 Direct URL 已经是 `https://sync.afraid.org/u/TOKEN/` 格式，请尝试使用 `https://freedns.afraid.org/dynamic/update.php?TOKEN&address={{combined_ip}}` 格式，其中 `TOKEN` 是 URL 中 `u/` 后面的一长串字符。

---

## 5. DuckDNS

DuckDNS 提供了简单的 API 来更新 IP 地址。

- **Method**: `GET`
- **URL**:
  ```text
  https://www.duckdns.org/update?domains=YOUR_DOMAIN&token=YOUR_TOKEN&ipv6={{combined_ip}}
  ```
- **Body**: `null` (留空)

### 参数说明

- **YOUR_DOMAIN**: 你的子域名（不包含 `.duckdns.org`，例如 `example`）。
- **YOUR_TOKEN**: 登录 [duckdns.org](https://www.duckdns.org/) 后在页面顶部显示的 Token。

---

## 6. deSEC.io

deSEC.io 支持通过标准 GET 请求更新 IPv6。

- **Method**: `GET`
- **URL**:
  ```text
  https://update.dedyn.io/?hostname=YOUR_FULL_DOMAIN&myipv6={{combined_ip}}
  ```
- **Headers**:
  ```json
  {
    "Authorization": "Token YOUR_TOKEN"
  }
  ```
- **Body**: `null` (留空)

### 参数说明

- **YOUR_FULL_DOMAIN**: 你的完整域名（例如 `example.dedyn.io` 或自定义域名 `ipv6.example.com`）。
  - **子域名**: 如果要更新子域名（如 `sub.example.dedyn.io`），直接在此处填写完整的子域名即可。deSEC 的 API 会自动处理。
- **YOUR_TOKEN**: 你的 deSEC Token（不是登录密码）。
  - Token 需要具有 DNS 管理权限。
  - **如何获取**: 登录 [desec.io](https://desec.io/) -> Token Management -> Create New Token。创建时记得保存 Token Secret，因为它只显示一次。
  - **注意**: deSEC 推荐使用 Header 进行认证，但也支持 URL 参数 `&username=YOUR_FULL_DOMAIN&password=YOUR_TOKEN`（不推荐，因为 Token 会暴露在 URL 日志中）。

---

## 7. YDNS

YDNS 提供了简单的 API 来更新 IP 地址。

- **Method**: `GET`
- **URL**:
  ```text
  https://ydns.io/api/v1/update/?host=YOUR_HOST&ip={{combined_ip}}
  ```
- **Headers**:
  ```json
  {
    "Authorization": "Basic YOUR_BASE64_AUTH"
  }
  ```
- **Body**: `null` (留空)

### 参数说明

- **YOUR_HOST**: 你的完整域名（例如 `example.ydns.io`）。
- **YOUR_BASE64_AUTH**: YDNS 要求使用 HTTP Basic Auth。你需要将 `username:password` 进行 Base64 编码。
  - **Username/Password**: 在 [ydns.io](https://ydns.io/) 登录后，进入 API 页面获取 API Username 和 Password。
  - **生成方法**:
    - Linux/Mac: `echo -n 'api_username:api_password' | base64`
    - 浏览器控制台: `btoa('api_username:api_password')`
  - **填入格式**: 如果生成的字符串是 `dXNlcjpwYXNz`，则 Header 中填入 `Basic dXNlcjpwYXNz`。

---

## 8. 通用 Webhook

用于对接自建服务、Server酱、Telegram Bot 等。

- **Method**: `POST` / `GET`
- **URL**: 你的 Webhook 地址。
- **Body**: 支持任意 JSON 格式。
  ```json
  {
    "content": "IPv6 Changed! New IP: {{combined_ip}}"
  }
  ```
