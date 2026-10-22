# CardCraft 游戏扩展与多品类演化设计指南 (Game Extensions & Spatial Evolution)

本文档系统性整理 CardCraft 架构在**“时空事件坐标”**、**“场景上下文压栈（地牢穿梭）”**与**“多尺度信息缩放（LOD 视口）”**上的理论模型，并详细列出**基于当前极简微内核能够实现的 8 大全新游戏品类**及其落地设计思路。

---

## 一、 核心理论模型：离散时空连续体 (Spatiotemporal Model)

CardCraft 的本质是一个**纯离散的时空事件物理模拟机**。任何复杂的游戏机制，在底层均可解构为以下三维几何映射：

```text
               Z 轴 (行内堆叠深度 Depth)
               ▲
               │  [卡牌 C] (顶层)
               │  [卡牌 B]
               │  [卡牌 A] (底层)
               └──────────────────────► X 轴 (终端行槽位 Row ID)
                  [ 1]  [ 2]  [ 3]  [ 4]
                   │     │
                   └─────┼────────────► 空间连接拓扑 (Link / Pipeline)
                         │
                         ▼
        T 轴 (逻辑时间刻度 / 事件坐标轴，由 game-update! dt 驱动)
```

1. **时间维（T 轴）**：
   * 不依赖现实时钟，由 `(game-update! dt)` 推进逻辑游标。
   * 游标前进撞上预设的“事件坐标”时（合成倒计时结束、月度终点、怪物攻击间隔），触发原子状态变更。
2. **空间维（X 轴与拓扑）**：
   * **行号槽位（Row）**：1D 离散网格。
   * **空间管道（Link）**：通过 `l <from> <to>` 将行连接为有向图，实现自动化物质转移。
   * **区域划分（Zones）**：通过槽位标签（如 `[H]` 手牌、`[B]` 战场、`[D]` 地牢）实现逻辑空间隔离。
3. **深度维（Z 轴）**：
   * 卡栈内的先后级联列表 `(A B C)`，决定了配方匹配、消耗优先级与装备依附关系。
4. **多尺度缩放（Zoom / LOD 视口）**：
   * **宏观视口 (Macro)**：大地图/战略层（行 = 区域/领地）。
   * **中观视口 (Meso)**：建造生产层（行 = 卡栈/流水线）。
   * **微观视口 (Micro)**：个体详情层（行 = 单个实体的器官/装备/属性）。
5. **世界上下文压栈（Context Push/Pop）**：
   * 类似操作系统的进程上下文切换或函数调用栈：
   * 触发进入地牢/飞船舱内时：`(push-scene! surface-world)`，屏幕切换到地牢。
   * 探索结束时：`(pop-scene!)`，携战利品返回地表。

---

## 二、 8 大衍生游戏品类落地设计方案

基于以上完全正交的极简微内核，无需引入任何物理引擎或复杂重构，即可衍生出以下 8 种完全不同类型的游戏：

---

### 1. 自动化流水线模拟（Factorio / 异星工厂）
* **核心玩法**：从手动挖矿发展到全自动化传送带与组装网络，最终发射火箭。
* **空间映射**：
  * 每行为一个工作台或传送带节点。
  * 使用空间管道 `link <from> <to>`（缩写 `l 1 2`）构建有向自动化拓扑网络。
* **时间与事件**：
  * 采矿机每 2 秒产出一次铁矿，通过 Link 自动推入下一行的熔炉；熔炉每 3 秒产出铁板，推入装配机。
* **终端 ASCII 呈现**：
  ```text
  [ 1] Miner IronPatch      [==......] 1.0s  ──► [ 2]
  [ 2] Furnace (Fuel: Coal) [====....] 1.5s  ──► [ 3]
  [ 3] Assembler Gear       [======..] 0.5s  ──► [ 4]
  [ 4] StorageChest (IronGear x24)
  ```
* **核心 CAD 指令**：`l 1 2`（连接管道）、`unlink 1`（拆除管道）、`filter 2 IronOre`（分流过滤）。

---

### 2. 牌组构建地牢肉鸽（Slay the Spire / 杀戮尖塔）
* **核心玩法**：每回合抽手牌，合理分配费用打出攻击、防御与技能，攻克层层地牢。
* **空间映射**：
  * **手牌区 `[H1] ~ [H5]`**：玩家每回合抽出的 5 张行动卡。
  * **英雄区 `[P1]`**：玩家本体卡栈（包含血量、护甲、遗物）。
  * **敌人区 `[E1] ~ [E3]`**：怪物卡栈（包含怪物血量、头顶意图与攻击倒计时）。
* **时间与事件**：
  * 回合制驱动，或即时 ATB 槽（Active Time Battle）：敌人头顶出现 `[===.] 1.5s (Intent: Attack 12)`，倒计时结束后自动对玩家造成伤害。
* **终端 ASCII 呈现**：
  ```text
  [P1] Ironclad (HP: 65/80 | Shield: 12 | Energy: 3/3)
  [E1] Cultist (HP: 24/48)   [====....] 2.0s (Intent: Attack 14)
  ----------------------------------------------------------------
  [H1] Strike (Cost: 1)      [H2] Defend (Cost: 1)
  [H3] Bash (Cost: 2)        [H4] IronWave (Cost: 1)
  ```
* **核心 CAD 指令**：`play 1 e1`（打出 H1 攻击 E1）、`end`（回合结束）。

---

### 3. 单行/多行塔防（Plants vs. Zombies / 字符塔防）
* **核心玩法**：怪物在行内从右侧向左侧老家推进，玩家在左侧建造防御设施进行阻击。
* **空间映射**：
  * 终端的每行代表一条进攻路线（Lane 1 ~ Lane 5）。
  * 行内的卡牌顺序代表物理空间位置（左侧是防御塔，右侧是正在逼近的怪物）。
* **时间与事件**：
  * 怪物每次 `tick` 沿行向左移动 1 格；防御塔倒计时射出子弹卡，命中同行的怪物并扣减生命。
* **终端 ASCII 呈现**：
  ```text
  [L1] Sunflower (Sun: 50) | Peashooter ──► ──► | Zombie (HP: 8)
  [L2] WallNut (HP: 40/40)                     | BucketheadZombie (HP: 20)
  [L3] Peashooter          ──►                 | ConeheadZombie (HP: 12)
  ```
* **核心 CAD 指令**：`plant pea 2`（在 2 号路部署豌豆射手）、`shovel 1`（铲除植物）。

---

### 4. 经典地牢探索与文字 MUD（Dungeon Crawler / Rogue）
* **核心玩法**：探索层层深渊，收集钥匙开门，避开陷阱，寻找下楼楼梯与远古神兵。
* **空间映射**：
  * 利用 **“场景上下文压栈（Push/Pop）”**：
  * 玩家将 `(Villager Sword)` 堆放到地面世界的 `DungeonPortal` 卡上，或者输入 `enter 4`；
  * 地面村庄推入后台，终端瞬间变成地牢第 1 层各个房间。
* **终端 ASCII 呈现**：
  ```text
  ================== DUNGEON LEVEL 2 (Floors Cleared: 1) ==================
  [D1] Party: Warrior (HP: 18/20) Mage (HP: 12/12)
  [D2] StoneDoor (Locked) Key: GoldenKey
  [D3] SkeletonArcher (HP: 8) TrappedChest
  [D4] StairsDown to Level 3
  ```
* **核心 CAD 指令**：`enter <id>`（进入地牢/房间）、`exit`（携战利品返回地面）、`unlock 2`（消耗钥匙开门）。

---

### 5. 烹饪与餐厅经营大亨（Overcooked / 潜水员戴夫）
* **核心玩法**：顾客订单不断流入，玩家调度厨师在备菜区、烹饪区、装盘区协同作业，防止食物烧焦。
* **空间映射**：
  * `[O1] ~ [O3]`：待处理的客户订单（超时扣分）。
  * `[W1] ~ [W4]`：料理台（切菜板、煎锅、烤箱、洗碗池）。
* **时间与事件**：
  * 煎牛排倒计时：3 秒从生肉变成熟肉；若在熟肉状态超过 2 秒未取下，自动氧化为焦炭卡！
* **终端 ASCII 呈现**：
  ```text
  [O1] Order #101: Burger (Req: Bun CookedMeat Cheese) [Time: 12s]
  [W1] Chef1 RawMeat CuttingBoard   [====....] 2.0s (Slicing)
  [W2] FryingPan CookedMeat         [========] ALERT: Burning in 1.8s!
  [W3] Plate Bun Cheese
  ```
* **核心 CAD 指令**：`chop 1`、`fry 2`、`serve 3 o1`。

---

### 6. 炼金术与元素合成解谜（Opus Magnum / Doodle God）
* **核心玩法**：给定水、火、土、气等基础元素，在极其严苛的空间槽位限制下，推导出复杂的高阶化合物或贤者之石。
* **空间映射**：
  * 极度限制槽位数量（例如全场只有 6 个空位）。
  * 必须精确规划堆叠顺序与中间废料的处置。
* **配方举例**：
  * `Fire + Water -> Steam`
  * `Earth + Fire -> Lava`
  * `Lava + Air -> Obsidian`
* **核心 CAD 指令**：`transmute <from> <to>`、`destroy <id>`（销毁废料）。

---

### 7. 生物遗传与生态圈模拟（Spore / 进化模拟器）
* **核心玩法**：培育物种，融合不同的基因卡牌，适应周期性变化的气候灾变。
* **空间映射**：
  * 每行为一个生物种群（包含基因卡栈：如 `[Carnivore, Fur, SharpClaws, Wings]`）。
* **时间与事件**：
  * **冰河世纪事件坐标**：未持有 `Fur`（毛皮）或 `Fat`（脂肪）基因的物种大批量减员；
  * **干旱事件坐标**：植物枯萎，草食生物被迫向肉食转化。
* **核心 CAD 指令**：`breed 1 2`（两物种交配并基于 Scheme 随机性产生突变后代）、`mutate 1 GeneRadiation`。

---

### 8. 科幻太空领地建设（RimWorld / 环世界）
* **核心玩法**：管理殖民飞船的舱段（生命维持、电力、水培农场、防御激光炮），应对太空海盗与太阳风暴。
* **空间映射（利用 Zoom 视口）**：
  * **宏观大地图（Zoom Out）**：扫描星系（行星、小行星带、废弃空间站）。
  * **中观空间站（Zoom Normal）**：各飞船舱室的卡牌运作。
  * **微观宇航员（Zoom In）**：检查殖民者的心理压力、氧气存量与装备。
* **终端 ASCII 呈现**：
  ```text
  [H1] LifeSupport (O2: 98%) PowerNode
  [H2] Hydroponics Colonist1 [====....] 3.5s (Harvest SpaceWheat)
  [H3] ShieldGenerator (Energy: 12/20)
  [ALERT] SolarFlare arriving in 45s! Prepare backup batteries.
  ```

---

## 三、 为什么同一套微内核能够支撑所有这些游戏？

这并非天方夜谭，而是源于 **Lisp/Scheme 的同构性（Homoiconicity）与组合性（Composability）**：

1. **实体统一性（Everything is a Card）**：
   * 无论是流水线的传送带、地牢里的骷髅弓箭手、还是塔防里的僵尸，其底层全部是带属性的 S 表达式符号，数据结构 100% 相同。
2. **行为统一性（Everything is Multiset Rewrite）**：
   * 采矿、合成、做菜、甚至战斗，其本质**全都是多重集替换规则（Multiset Rewriting Rules）**：
     * `(A + B -> C + D in T seconds)`
     * 采矿是它，打怪（`Warrior + Goblin -> Warrior(HP-2) + Bones + Gold`）也是它。
3. **空间与时间正交（Time × Space Decoupled）**：
   * 引擎底层只负责：
     * 时钟走 `dt` 时更新倒计时；
     * 输入命令时按照规则移动列表元素；
     * 渲染时将列表以只读 JSON 输出。

**这意味着：你不需要改动一行业已编译的 Rust 底层代码，只需挂载不同的 `game.scm` 规则脚本，就能将终端一键切换成完全不同品类的神作！**
