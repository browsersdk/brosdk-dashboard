# 跨境电商运营中台规划

## 产品定位

BroSDK Dashboard 不宜直接扩展成传统“大 ERP”。更合适的方向是以多环境指纹浏览器为底座，演进为面向跨境卖家的轻量运营中台：

```text
多环境指纹浏览器 + 店铺环境绑定 + 订单发货管理 + 商品/SKU 同步 + 受控 AI Agent
```

这个定位保留当前项目的差异化能力：每个店铺账号可以绑定独立 `envId`，API 能处理的流程走平台连接器，API 缺口、授权困难或风控敏感的流程用绑定环境打开后台，由用户或 Agent 半自动完成。

## 边界

当前阶段不做完整 ERP：

- 不先做采购、财务、复杂仓库、利润核算、员工权限和多租户。
- 不把本地数据库变成平台商品、订单或店铺账号的最终事实来源。
- 不用浏览器自动化绕过平台 API、平台风控或授权边界。
- 不保存平台 token、买家隐私、物流凭据或店铺敏感信息的明文。

优先做轻量运营闭环：

- 店铺账号与浏览器环境绑定。
- 多平台订单拉取、未发货筛选、物流单号回传。
- 统一 SKU 与平台商品映射。
- 库存、价格和基础商品资料同步。
- 平台 API 失败时提供可恢复 operation、人工打开后台和 Agent 辅助。

## 核心对象

| 对象 | 主键建议 | 说明 |
| --- | --- | --- |
| 店铺 `commerce_store` | `storeId` | 平台、店铺名、绑定 `envId`、授权状态、默认仓/物流策略 |
| 平台授权 `commerce_auth` | `authId` | OAuth/token/刷新状态，密钥进入平台安全存储 |
| 商品 `commerce_product` | `productId` | 本地统一 SPU，不直接覆盖平台商品事实 |
| SKU `commerce_sku` | `skuId` | 本地统一 SKU、库存策略、平台映射键 |
| 平台商品映射 `commerce_listing` | `listingId` | storeId + platform listing/item/product id |
| 订单 `commerce_order` | `orderId` | storeId + platform order id，保留同步新鲜度 |
| 发货 `commerce_fulfillment` | `fulfillmentId` | 物流商、单号、发货状态、回传结果 |
| 同步任务 `commerce_sync_job` | `operationId` | 接入既有 Manager operation，可重试、可审计 |

## 平台连接器

连接器使用统一接口，具体平台实现逐步接入：

```text
pullOrders(storeId, since)
pushTracking(storeId, orderId, tracking)
syncInventory(storeId, skuId, quantity)
syncPrice(storeId, skuId, price)
syncProduct(storeId, productId)
refreshAuth(storeId)
openSellerConsole(storeId, envId)
```

优先平台：

- Amazon：Selling Partner API，订单、库存、履约优先。
- Shopify：Admin API，订单、商品、库存与 fulfillment 优先。
- eBay：Sell Inventory / Fulfillment APIs。
- TikTok Shop：Open API，订单、商品、履约优先。

平台 API 返回的数据只作为同步快照进入本地缓存；平台侧仍是订单、库存和刊登状态的事实来源。

## AI Agent 边界

Agent 只通过 Manager 暴露的 commerce tools 工作，不能直接持有平台 token 或店铺后台密码。

推荐工具分层：

- `commerce.store.list/get/open`
- `commerce.order.list/get/sync`
- `commerce.fulfillment.create/push`
- `commerce.sku.list/get/sync_inventory`
- `commerce.listing.sync_price/sync_product`
- `commerce.operation.retry`

涉及写操作时沿用现有 Agent 策略：

- Chat 默认只读。
- Agent 默认逐次批准。
- 自动执行必须是会话级显式选择。
- 每一步进入 operation，记录脱敏参数、目标店铺、目标 `envId` 和平台响应摘要。

## 分阶段路线

### 阶段 35：店铺与环境绑定

目标：让 Dashboard 明确知道哪个平台店铺对应哪个浏览器环境。

交付：

- 新增店铺列表、店铺详情和绑定 `envId`。
- 支持平台、店铺名、授权状态、默认环境、备注。
- 店铺详情可一键打开绑定环境。
- 不接入平台 API，先完成本地模型、UI 和 operation 轨迹。

### 阶段 36：订单与发货工作台

目标：先解决跨境卖家最高频的未发货订单处理。

交付：

- 订单列表、状态筛选、店铺筛选、未发货视图。
- 发货信息录入、物流单号校验、回传任务。
- 同步失败可重试，敏感字段脱敏。
- 支持 mock connector 和至少一个真实平台 connector 的只读拉单。

### 阶段 37：SKU 与库存同步

目标：建立统一 SKU 与平台 listing 映射，先覆盖库存和价格。

交付：

- SKU 中心、平台 listing 映射。
- 库存与价格同步任务。
- 平台差异以 connector adapter 消化，不把平台私有字段泄漏到通用 UI。

### 阶段 38：商品资料与刊登同步

目标：在订单/库存稳定后再做更复杂的商品资料与刊登。

交付：

- SPU/SKU 商品资料、图片引用、标题、属性模板。
- 平台刊登草稿、预检、提交和失败报告。
- 高风险字段修改必须人工批准。

### 阶段 39：Commerce Agent

目标：让 AI 在受控工具边界内处理日常运营问题。

交付：

- “检查今天未发货订单”“同步某 SKU 库存”“打开店铺后台查看异常订单”等意图。
- Agent 步骤显示店铺、平台、envId、工具名和脱敏参数。
- 自动模式只允许低风险读取和明确白名单写操作。

## 验收原则

- 每个店铺、订单、SKU、listing 都必须带平台来源和外部 id。
- 本地缓存必须有同步时间、新鲜度和失败原因。
- 平台 token、买家手机号、完整地址、Cookie、店铺后台凭据不得进入日志、截图、operation 明文和 AI prompt。
- 所有写操作必须可审计、可重试、可失败回滚到明确状态。
- API 缺口由绑定环境打开后台补位，不伪装成已完成的自动化。
