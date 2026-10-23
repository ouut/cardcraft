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

### 2. 核心配方一览 (Stacklands 原版精细还原)
```scheme
(define *recipes*
  '(;; 1. 农业与资源采集
    ("Harvest Berry"  ("Villager" "BerryBush") 4.0 ("Berry") ())
    ("Chop Tree"      ("Villager" "Tree") 5.0 ("Wood") ())
    ("Mine Rock"      ("Villager" "Rock") 5.0 ("Stone") ())
    ("Mine Iron"      ("Villager" "IronDeposit") 6.0 ("IronOre") ())

    ;; 2. 烹饪与加工 (营火烹饪)
    ("Cook Berry"     ("Campfire" "Berry") 3.0 ("CookedBerry") ("Berry"))
    ("Cook Meat"      ("Campfire" "RawMeat") 4.0 ("CookedMeat") ("RawMeat"))
    ("Fruit Salad"    ("Campfire" "Berry" "Apple") 4.0 ("FruitSalad") ("Berry" "Apple"))
    ("Meat Stew"      ("Campfire" "CookedMeat" "Carrot") 5.0 ("Stew") ("CookedMeat" "Carrot"))

    ;; 3. 种植与农业循环 (Soil / Garden / Farm)
    ("Plant Berry"    ("Villager" "Soil" "Berry") 6.0 ("BerryBush") ("Berry"))
    ("Plant Tree"     ("Villager" "Soil" "Apple") 8.0 ("Tree") ("Apple"))
    ("Farm Carrots"   ("Villager" "Farm" "Carrot") 5.0 ("Carrot" "Carrot") ())
    ("Garden Berries" ("Villager" "Garden" "Berry") 5.0 ("Berry" "Berry") ())

    ;; 4. 材料加工
    ("Make Stick"     ("Villager" "Wood") 3.0 ("Stick" "Stick") ("Wood"))
    ("Make Plank"     ("Villager" "Wood" "Wood") 4.0 ("Plank") ("Wood" "Wood"))
    ("Make Brick"     ("Villager" "Stone" "Stone") 4.0 ("Brick") ("Stone" "Stone"))
    ("Smelt Iron"     ("Smelter" "IronOre" "Wood") 5.0 ("IronBar") ("IronOre" "Wood"))

    ;; 5. 装备打造与职业转职
    ("Craft Spear"    ("Villager" "Wood" "Flint") 4.0 ("Spear") ("Wood" "Flint"))
    ("Equip Militia"  ("Villager" "Spear") 2.0 ("Militia") ("Villager" "Spear"))
    ("Craft Sword"    ("Villager" "IronBar" "Stick") 5.0 ("Sword") ("IronBar" "Stick"))
    ("Train Swordsman" ("Villager" "Sword") 3.0 ("Swordsman") ("Villager" "Sword"))
    ("Tame Wolf"      ("Wolf" "Bone") 5.0 ("Dog") ("Wolf" "Bone"))

    ;; 6. 建筑建造
    ("Build Campfire" ("Villager" "Wood" "Wood" "Stone") 6.0 ("Campfire") ("Wood" "Wood" "Stone"))
    ("Build Shed"     ("Villager" "Wood" "Stone" "Stone") 6.0 ("Shed") ("Wood" "Stone" "Stone"))
    ("Build House"    ("Villager" "Wood" "Wood" "Stone" "Stone") 8.0 ("House") ("Wood" "Wood" "Stone" "Stone"))
    ("Build Garden"   ("Villager" "Soil" "Wood" "Stone") 6.0 ("Garden") ("Soil" "Wood" "Stone"))
    ("Build Farm"     ("Villager" "Garden" "Plank" "Brick") 8.0 ("Farm") ("Garden" "Plank" "Brick"))
    ("Build Quarry"   ("Villager" "Stone" "Stone" "Wood") 8.0 ("Quarry") ("Stone" "Stone" "Wood"))
    ("Build LumberCamp" ("Villager" "Wood" "Wood" "Stone") 8.0 ("LumberCamp") ("Wood" "Wood" "Stone"))
    ("Build Smelter"  ("Villager" "Brick" "Brick" "Flint") 8.0 ("Smelter") ("Brick" "Brick" "Flint"))
    ("Build Warehouse" ("Villager" "Plank" "Brick" "IronBar") 10.0 ("Warehouse") ("Plank" "Brick" "IronBar"))

    ;; 7. 持续性工业生产 (Automation)
    ("Work Quarry"    ("Villager" "Quarry") 5.0 ("Stone") ())
    ("Work LumberCamp" ("Villager" "LumberCamp") 5.0 ("Wood") ())

    ;; 8. 人口繁衍与成长
    ("Breed Villager" ("Villager" "Villager" "House") 10.0 ("Baby") ())
    ("Grow Baby"      ("Baby") 10.0 ("Villager") ("Baby"))
    ("Grow Baby House" ("Baby" "House") 6.0 ("Villager") ("Baby"))

    ;; 9. 怪物战斗与狩猎
    ("Fight Rat"      ("Villager" "Rat") 4.0 ("RawMeat" "Coin") ("Rat"))
    ("Hunt Rat"       ("Militia" "Rat") 2.0 ("RawMeat" "Coin") ("Rat"))
    ("Fight Goblin"   ("Militia" "Goblin") 4.0 ("Coin" "Bone") ("Goblin"))
    ("Slay Goblin"    ("Swordsman" "Goblin") 2.5 ("Coin" "Spear") ("Goblin"))
    ("Fight Skeleton" ("Swordsman" "Skeleton") 4.0 ("Sword" "Bone") ("Skeleton"))
    ("Hunt Wolf"      ("Militia" "Wolf") 5.0 ("RawMeat" "Bone") ("Wolf"))))
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
