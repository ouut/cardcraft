# CardCraft Scheme 规则与 Mod 编写指南 (Scheme DSL Guide)

CardCraft 的游戏规则引擎完全基于 **Steel Scheme** 构建。所有的合成表、卡牌属性、价值体系均由 S-expression（S 表达式）声明。你可以非常轻松地通过修改或追加 Scheme 代码来扩充游戏内容。

---

## 一、 卡牌定义与数据属性

在当前版本中，卡牌通过符号或字符串进行唯一标识（如 `"Villager"`、`"BerryBush"`、`"Wood"`）。

卡牌的数值属性由查询函数驱动：

### 1. 饱食度属性 (`card-food-value`)
当一个月度周期结束时，系统会扫描所有卡牌的饱食度，自动消耗满足全村人口所需的食物：
```scheme
(define (card-food-value card)
  (cond
    [(equal? card "Berry") 1]
    [(equal? card "Apple") 1]
    [(equal? card "CookedBerry") 2]
    [else 0]))
```

### 2. 出售价值属性 (`card-sell-value`)
玩家通过 `x <slot>` 命令卖出卡牌时获得的金币数：
```scheme
(define (card-sell-value card)
  (cond
    [(equal? card "Coin") 1]
    [(equal? card "Berry") 1]
    [(equal? card "Wood") 1]
    [(equal? card "Stone") 1]
    [(equal? card "CookedBerry") 2]
    [(equal? card "Campfire") 3]
    [(equal? card "House") 5]
    [(equal? card "Shed") 4]
    [(equal? card "BerryBush") 2]
    [(equal? card "Tree") 2]
    [(equal? card "Rock") 2]
    [else 0])) ; 价值为 0 时无法被出售
```

---

## 二、 配方系统（Recipe DSL）

### 1. 配方数据结构
配方表 `*recipes*` 是一个 S 表达式列表，每一个配方的结构为五元组：

```scheme
("配方名称" 
 (原料卡列表) 
 耗时秒数 
 (生产产物列表) 
 (被消耗的原味卡列表))
```

* **原料卡列表 (`inputs`)**：触发此配方所需的最小卡牌集合（多重集）。
* **耗时秒数 (`duration`)**：进度条需要倒计时的秒数。
* **生产产物列表 (`outputs`)**：倒计时结束后弹出的新卡牌。
* **被消耗的卡牌列表 (`consumed`)**：
  * 若某张卡在合成中作为**工具/工人**保留（如采果时的村民与果丛），则**不放入**此列表；
  * 若某张卡作为**消耗品/建筑材料**销毁（如造营火时的木头与石头），则**必须放入**此列表。

### 2. 核心配方一览
```scheme
(define *recipes*
  '(;; 1. 农业采集：保留村民和果丛，每 4 秒生成 1 颗浆果
    ("Harvest Berry" ("Villager" "BerryBush") 4.0 ("Berry") ())

    ;; 2. 砍伐与采石：
    ("Chop Tree"     ("Villager" "Tree") 5.0 ("Wood") ())
    ("Mine Rock"     ("Villager" "Rock") 5.0 ("Stone") ())

    ;; 3. 烹饪食物：营火保留，生浆果被消耗，产生高饱食度熟浆果
    ("Cook Berry"    ("Campfire" "Berry") 3.0 ("CookedBerry") ("Berry"))

    ;; 4. 建造营火：村民保留，2木1石被消耗
    ("Build Campfire" ("Villager" "Wood" "Wood" "Stone") 6.0 ("Campfire") ("Wood" "Wood" "Stone"))

    ;; 5. 建造棚屋：扩充卡牌上限（+5上限）
    ("Build Shed"     ("Villager" "Wood" "Stone" "Stone") 6.0 ("Shed") ("Wood" "Stone" "Stone"))

    ;; 6. 建造房屋：用于人口繁衍
    ("Build House"    ("Villager" "Wood" "Wood" "Stone" "Stone") 8.0 ("House") ("Wood" "Wood" "Stone" "Stone"))

    ;; 7. 人口繁衍：2个村民在房屋中培育婴儿
    ("Breed Villager" ("Villager" "Villager" "House") 10.0 ("Baby") ())

    ;; 8. 婴儿成长：婴儿成长为成年村民
    ("Grow Baby"      ("Baby") 10.0 ("Villager") ("Baby"))))
```

---

## 三、 多重集模式匹配原理

在 `scripts/core.scm` 中，匹配算法使用函数式列表递归实现：

```scheme
(define (multiset-subset? sub super)
  (cond
    [(null? sub) #t]
    [else
     (let ([item (car sub)])
       (if (member item super)
           (multiset-subset? (cdr sub) (remove-one item super))
           #f))]))
```

* **优势**：
  * **乱序无关**：无论玩家先放木头还是先放石头，只要多重集满足即自动匹配。
  * **超集兼容**：如果玩家在一个栈里放了 3 块木头和 1 块石头，依然能正常识别出需要 2 块木头的配方，合成完成后只消耗 2 块，多余的 1 块木头继续保留。

---

## 四、 编写自定义 Mod 示例

假设你想为游戏新增**冶铁与锻造武器系统**：

在控制台中，你可以直接输入以下代码动态注册新配方：

```scheme
;; 1. 注册新配方：村民 + 石头 + 木头 = 冶炼炉 (Furnace)
(set! *recipes* 
  (cons '("Build Furnace" ("Villager" "Stone" "Stone" "Wood") 6.0 ("Furnace") ("Stone" "Stone" "Wood"))
        *recipes*))

;; 2. 注册锻造配方：村民 + 冶炼炉 + 铁矿 = 铁剑 (Sword)
(set! *recipes*
  (cons '("Forge Sword" ("Villager" "Furnace" "IronOre") 5.0 ("Sword") ("IronOre"))
        *recipes*))

;; 3. 动态刷出一块铁矿用于测试
(spawn-card! "IronOre")
```

此时游戏无需重启，你的新配方和新卡牌立即生效！
