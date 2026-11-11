//! 本地屏蔽规则引擎（对应 `service/local_block_rule_service.dart`）。
//!
//! 读 `block_rule` 表的四列（target/attribute/pattern/expression），
//! 枚举码与 Dart 版一致，因此可以直接用旧数据库里的规则。
//!
//! 当前实现覆盖**画廊列表**的规则（标题/标签/上传者/gid）；评论规则
//! （userName/userId/score/content）保留枚举与匹配逻辑，等评论页落地再接。

use regex::Regex;

/// 规则作用对象（与 Dart `LocalBlockTargetEnum` 的 code 一致）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockTarget {
    Gallery,
    Comment,
}

impl BlockTarget {
    pub fn from_code(code: i64) -> Option<Self> {
        match code {
            0 => Some(Self::Gallery),
            1 => Some(Self::Comment),
            _ => None,
        }
    }
}

/// 规则属性（与 Dart `LocalBlockAttributeEnum` 的 code 一致）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockAttribute {
    Title,
    Tag,
    Uploader,
    Gid,
    UserName,
    UserId,
    Score,
    Content,
}

impl BlockAttribute {
    pub fn from_code(code: i64) -> Option<Self> {
        match code {
            0 => Some(Self::Title),
            10 => Some(Self::Tag),
            20 => Some(Self::Uploader),
            30 => Some(Self::Gid),
            100 => Some(Self::UserName),
            110 => Some(Self::UserId),
            120 => Some(Self::Score),
            130 => Some(Self::Content),
            _ => None,
        }
    }
}

/// 匹配方式（与 Dart `LocalBlockPatternEnum` 的 code 一致）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockPattern {
    Equal,
    GreaterThan,
    GreaterOrEqual,
    LessThan,
    LessOrEqual,
    Contain,
    NotContain,
    Regex,
}

impl BlockPattern {
    pub fn from_code(code: i64) -> Option<Self> {
        match code {
            0 => Some(Self::Equal),
            10 => Some(Self::GreaterThan),
            20 => Some(Self::GreaterOrEqual),
            30 => Some(Self::LessThan),
            40 => Some(Self::LessOrEqual),
            50 => Some(Self::Contain),
            60 => Some(Self::NotContain),
            70 => Some(Self::Regex),
            _ => None,
        }
    }
}

/// `block_rule` 表的一行
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockRule {
    pub group_id: String,
    pub target: BlockTarget,
    pub attribute: BlockAttribute,
    pub pattern: BlockPattern,
    pub expression: String,
}

impl BlockRule {
    /// 从数据库行解析；枚举码不认识的规则直接丢弃（旧版本/未来版本）
    pub fn from_row(group_id: String, target: i64, attribute: i64, pattern: i64, expression: String) -> Option<Self> {
        Some(Self {
            group_id,
            target: BlockTarget::from_code(target)?,
            attribute: BlockAttribute::from_code(attribute)?,
            pattern: BlockPattern::from_code(pattern)?,
            expression,
        })
    }

    /// 单个值是否命中（**大小写敏感**，与 Dart 的 `==` / `contains` / `RegExp` 一致）
    pub fn matches_str(&self, value: &str) -> bool {
        match self.pattern {
            BlockPattern::Equal => value == self.expression,
            BlockPattern::Contain => value.contains(&self.expression),
            BlockPattern::NotContain => !value.contains(&self.expression),
            BlockPattern::Regex => Regex::new(&self.expression).map(|re| re.is_match(value)).unwrap_or(false),
            // 数值模式只对 score 有意义，字符串值上不命中
            _ => false,
        }
    }

    /// 对一组候选值求值，量词与 Dart handler 对齐：
    /// - Equal / Contain / Regex：任一值命中即命中（`any`）
    /// - NotContain：所有值都不包含表达式才命中（`every`）——Dart 的 tag handler 就是这样
    pub fn matches_values(&self, values: &[String]) -> bool {
        if values.is_empty() {
            // Dart 对空值列表用 `every`：NotContain 恒真（匿名画廊会被 uploader notContain 规则屏蔽），
            // 其余模式 `any` 恒假。这里逐字对齐，保证同一条旧规则两版行为一致。
            return matches!(self.pattern, BlockPattern::NotContain);
        }
        match self.pattern {
            BlockPattern::NotContain => values.iter().all(|v| self.matches_str(v)),
            _ => values.iter().any(|v| self.matches_str(v)),
        }
    }

    /// 数值规则（score）
    pub fn matches_score(&self, score: i64) -> bool {
        let Ok(target) = self.expression.trim().parse::<i64>() else {
            return false;
        };
        match self.pattern {
            BlockPattern::GreaterThan => score > target,
            BlockPattern::GreaterOrEqual => score >= target,
            BlockPattern::LessThan => score < target,
            BlockPattern::LessOrEqual => score <= target,
            _ => false,
        }
    }
}

/// 一组规则的集合（按 group 分组在 UI 里展示；匹配时全量跑）
#[derive(Debug, Default, Clone)]
pub struct BlockRuleSet {
    pub rules: Vec<BlockRule>,
}

impl BlockRuleSet {
    pub fn new(rules: Vec<BlockRule>) -> Self {
        Self { rules }
    }

    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    /// 画廊是否应被屏蔽。
    ///
    /// 语义与 Dart 一致：**同一 group 内所有规则都命中才屏蔽，组与组之间任一命中即屏蔽**。
    pub fn blocks_gallery(&self, gallery: &crate::model::Gallery) -> bool {
        let mut groups: Vec<(&str, Vec<&BlockRule>)> = Vec::new();
        for rule in &self.rules {
            match groups.iter_mut().find(|(id, _)| *id == rule.group_id.as_str()) {
                Some((_, list)) => list.push(rule),
                None => groups.push((rule.group_id.as_str(), vec![rule])),
            }
        }

        groups.iter().any(|(_, rules)| {
            rules.iter().all(|rule| {
                let values = gallery_values(gallery, rule.attribute);
                match rule.attribute {
                    BlockAttribute::Score => false,
                    _ => rule.matches_values(&values),
                }
            })
        })
    }
}

/// 某属性在画廊上的候选值（与 Dart 的 attribute getter 对应）
fn gallery_values(gallery: &crate::model::Gallery, attribute: BlockAttribute) -> Vec<String> {
    match attribute {
        BlockAttribute::Title => vec![gallery.title.clone()],
        BlockAttribute::Uploader => gallery.uploader.clone().map(|u| vec![u]).unwrap_or_default(),
        BlockAttribute::Gid => vec![gallery.gid().to_string()],
        BlockAttribute::Tag => gallery
            .tags
            .iter()
            .flat_map(|(_, tags)| {
                tags.iter().flat_map(|t| [t.tag_data.full(), t.tag_data.key.clone()])
            })
            .collect(),
        // 评论属性不作用于画廊
        _ => Vec::new(),
    }
}

/// 从数据库读全部规则（表不存在时返回空集，不影响浏览）
pub fn load_rules(conn: &rusqlite::Connection) -> crate::db::Result<BlockRuleSet> {
    let mut stmt = conn.prepare("SELECT group_id, target, attribute, pattern, expression FROM block_rule")?;
    let rows = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?, row.get::<_, i64>(2)?, row.get::<_, i64>(3)?, row.get::<_, String>(4)?))
    })?;

    let mut rules = Vec::new();
    for row in rows {
        let (group, target, attribute, pattern, expression) = row?;
        if let Some(rule) = BlockRule::from_row(group, target, attribute, pattern, expression) {
            rules.push(rule);
        }
    }
    Ok(BlockRuleSet::new(rules))
}

/// 插入一条规则，返回新行 id
pub fn insert_rule(conn: &rusqlite::Connection, rule: &BlockRule) -> crate::db::Result<i64> {
    conn.execute(
        "INSERT INTO block_rule (group_id, target, attribute, pattern, expression) VALUES (?1, ?2, ?3, ?4, ?5)",
        (
            &rule.group_id,
            match rule.target {
                BlockTarget::Gallery => 0i64,
                BlockTarget::Comment => 1i64,
            },
            attribute_code(rule.attribute),
            pattern_code(rule.pattern),
            &rule.expression,
        ),
    )?;
    Ok(conn.last_insert_rowid())
}

/// 删除一条规则
pub fn delete_rule(conn: &rusqlite::Connection, id: i64) -> crate::db::Result<()> {
    conn.execute("DELETE FROM block_rule WHERE id = ?1", (id,))?;
    Ok(())
}

/// 带 id 的规则行（设置页展示/删除用）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredRule {
    pub id: i64,
    pub rule: BlockRule,
}

pub fn all_rules(conn: &rusqlite::Connection) -> crate::db::Result<Vec<StoredRule>> {
    let mut stmt = conn.prepare("SELECT id, group_id, target, attribute, pattern, expression FROM block_rule ORDER BY id")?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, i64>(2)?,
            row.get::<_, i64>(3)?,
            row.get::<_, i64>(4)?,
            row.get::<_, String>(5)?,
        ))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (id, group, target, attribute, pattern, expression) = row?;
        if let Some(rule) = BlockRule::from_row(group, target, attribute, pattern, expression) {
            out.push(StoredRule { id, rule });
        }
    }
    Ok(out)
}

fn attribute_code(attribute: BlockAttribute) -> i64 {
    match attribute {
        BlockAttribute::Title => 0,
        BlockAttribute::Tag => 10,
        BlockAttribute::Uploader => 20,
        BlockAttribute::Gid => 30,
        BlockAttribute::UserName => 100,
        BlockAttribute::UserId => 110,
        BlockAttribute::Score => 120,
        BlockAttribute::Content => 130,
    }
}

fn pattern_code(pattern: BlockPattern) -> i64 {
    match pattern {
        BlockPattern::Equal => 0,
        BlockPattern::GreaterThan => 10,
        BlockPattern::GreaterOrEqual => 20,
        BlockPattern::LessThan => 30,
        BlockPattern::LessOrEqual => 40,
        BlockPattern::Contain => 50,
        BlockPattern::NotContain => 60,
        BlockPattern::Regex => 70,
    }
}

/// 一条规则的可读描述（设置页列表用）
pub fn describe(rule: &BlockRule) -> String {
    let attribute = match rule.attribute {
        BlockAttribute::Title => "title",
        BlockAttribute::Tag => "tag",
        BlockAttribute::Uploader => "uploader",
        BlockAttribute::Gid => "gid",
        BlockAttribute::UserName => "user",
        BlockAttribute::UserId => "uid",
        BlockAttribute::Score => "score",
        BlockAttribute::Content => "comment",
    };
    let pattern = match rule.pattern {
        BlockPattern::Equal => "=",
        BlockPattern::GreaterThan => ">",
        BlockPattern::GreaterOrEqual => ">=",
        BlockPattern::LessThan => "<",
        BlockPattern::LessOrEqual => "<=",
        BlockPattern::Contain => "contain",
        BlockPattern::NotContain => "notContain",
        BlockPattern::Regex => "regex",
    };
    format!("{attribute} {pattern} {}", rule.expression)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Gallery, GalleryTag, GalleryUrl, TagData, TagMap};

    fn rule(attribute: BlockAttribute, pattern: BlockPattern, expression: &str) -> BlockRule {
        BlockRule { group_id: "default".into(), target: BlockTarget::Gallery, attribute, pattern, expression: expression.into() }
    }

    fn gallery(title: &str, uploader: Option<&str>, tags: &[(&str, &str)]) -> Gallery {
        let mut tag_map = TagMap::new();
        for (ns, key) in tags {
            tag_map.push(GalleryTag::new(TagData::new(*ns, *key)));
        }
        Gallery {
            gallery_url: GalleryUrl::new(true, 42, "aaaaaaaaaa"),
            title: title.into(),
            category: "Misc".into(),
            cover: Default::default(),
            page_count: Some(10),
            rating: 0.0,
            has_rated: false,
            favorite_tag_index: None,
            favorite_tag_name: None,
            language: None,
            uploader: uploader.map(str::to_string),
            publish_time: String::new(),
            is_expunged: false,
            tags: tag_map,
        }
    }

    #[test]
    fn enum_codes_match_dart() {
        assert_eq!(BlockTarget::from_code(0), Some(BlockTarget::Gallery));
        assert_eq!(BlockTarget::from_code(1), Some(BlockTarget::Comment));
        assert_eq!(BlockAttribute::from_code(10), Some(BlockAttribute::Tag));
        assert_eq!(BlockAttribute::from_code(20), Some(BlockAttribute::Uploader));
        assert_eq!(BlockAttribute::from_code(130), Some(BlockAttribute::Content));
        assert_eq!(BlockPattern::from_code(50), Some(BlockPattern::Contain));
        assert_eq!(BlockPattern::from_code(70), Some(BlockPattern::Regex));
        assert_eq!(BlockPattern::from_code(999), None);
    }

    fn grouped(group: &str, rules: Vec<BlockRule>) -> BlockRuleSet {
        BlockRuleSet::new(rules.into_iter().map(|mut r| { r.group_id = group.into(); r }).collect())
    }

    #[test]
    fn title_rules_are_case_sensitive_like_dart() {
        let g = gallery("Boring Gallery", None, &[]);
        assert!(BlockRuleSet::new(vec![rule(BlockAttribute::Title, BlockPattern::Contain, "Boring")]).blocks_gallery(&g));
        // Dart 用 `contains`，区分大小写：小写表达式不命中
        assert!(!BlockRuleSet::new(vec![rule(BlockAttribute::Title, BlockPattern::Contain, "boring")]).blocks_gallery(&g));
        assert!(BlockRuleSet::new(vec![rule(BlockAttribute::Title, BlockPattern::Regex, "^Boring")]).blocks_gallery(&g));
        // notContain：标题不包含表达式时屏蔽
        assert!(BlockRuleSet::new(vec![rule(BlockAttribute::Title, BlockPattern::NotContain, "zzz")]).blocks_gallery(&g));
        assert!(!BlockRuleSet::new(vec![rule(BlockAttribute::Title, BlockPattern::NotContain, "Gallery")]).blocks_gallery(&g));
    }

    #[test]
    fn group_is_and_between_groups_is_or() {
        let g = gallery("Spam Gallery", Some("Spammer"), &[("female", "sole female")]);
        // 同组两条规则：都命中才屏蔽
        let both = grouped(
            "g1",
            vec![
                rule(BlockAttribute::Title, BlockPattern::Contain, "Spam"),
                rule(BlockAttribute::Uploader, BlockPattern::Equal, "Spammer"),
            ],
        );
        assert!(both.blocks_gallery(&g), "组内全部命中 → 屏蔽");

        let only_one = grouped(
            "g1",
            vec![
                rule(BlockAttribute::Title, BlockPattern::Contain, "Spam"),
                rule(BlockAttribute::Uploader, BlockPattern::Equal, "SomeoneElse"),
            ],
        );
        assert!(!only_one.blocks_gallery(&g), "组内只命中一条 → 不屏蔽");

        // 不同组：任一组成立即屏蔽
        let mut set = grouped("g1", vec![rule(BlockAttribute::Uploader, BlockPattern::Equal, "nope")]);
        set.rules.extend(grouped("g2", vec![rule(BlockAttribute::Title, BlockPattern::Contain, "Spam")]).rules);
        assert!(set.blocks_gallery(&g), "组间是 OR");
    }

    #[test]
    fn tag_rules_quantifiers() {
        let g = gallery("x", Some("Spammer"), &[("female", "sole female"), ("male", "sole male")]);
        // Contain：任一标签命中即可
        assert!(BlockRuleSet::new(vec![rule(BlockAttribute::Tag, BlockPattern::Contain, "female:sole female")]).blocks_gallery(&g));
        // NotContain：所有标签都不包含表达式才命中（Dart 的 every 语义）
        assert!(!BlockRuleSet::new(vec![rule(BlockAttribute::Tag, BlockPattern::NotContain, "sole")]).blocks_gallery(&g));
        assert!(BlockRuleSet::new(vec![rule(BlockAttribute::Tag, BlockPattern::NotContain, "loli")]).blocks_gallery(&g));
        // Equal 对完整 `ns:key` 与裸 key 都生效
        assert!(BlockRuleSet::new(vec![rule(BlockAttribute::Tag, BlockPattern::Equal, "sole male")]).blocks_gallery(&g));
    }

    #[test]
    fn uploader_rules_case_sensitive() {
        let g = gallery("x", Some("Spammer"), &[]);
        assert!(BlockRuleSet::new(vec![rule(BlockAttribute::Uploader, BlockPattern::Equal, "Spammer")]).blocks_gallery(&g));
        assert!(!BlockRuleSet::new(vec![rule(BlockAttribute::Uploader, BlockPattern::Equal, "spammer")]).blocks_gallery(&g));
    }

    #[test]
    fn gid_and_invalid_regex() {
        let g = gallery("x", None, &[]);
        assert!(BlockRuleSet::new(vec![rule(BlockAttribute::Gid, BlockPattern::Equal, "42")]).blocks_gallery(&g));
        // 非法正则不应崩溃，也不应命中
        assert!(!BlockRuleSet::new(vec![rule(BlockAttribute::Title, BlockPattern::Regex, "[")]).blocks_gallery(&g));
    }

    #[test]
    fn not_contain_on_empty_values_blocks_like_dart() {
        // 匿名画廊（无 uploader）：Dart 的 every 对空列表为真 → 被屏蔽
        let anonymous = gallery("x", None, &[]);
        assert!(
            BlockRuleSet::new(vec![rule(BlockAttribute::Uploader, BlockPattern::NotContain, "someone")])
                .blocks_gallery(&anonymous),
            "空值 + NotContain 应命中（与 Dart every 语义一致）"
        );
        // 其余模式对空值不命中
        assert!(!BlockRuleSet::new(vec![rule(BlockAttribute::Uploader, BlockPattern::Equal, "someone")])
            .blocks_gallery(&anonymous));
        assert!(!BlockRuleSet::new(vec![rule(BlockAttribute::Uploader, BlockPattern::Contain, "someone")])
            .blocks_gallery(&anonymous));
    }

    #[test]
    fn score_rules() {
        assert!(rule(BlockAttribute::Score, BlockPattern::LessThan, "0").matches_score(-1));
        assert!(rule(BlockAttribute::Score, BlockPattern::GreaterOrEqual, "5").matches_score(5));
        assert!(!rule(BlockAttribute::Score, BlockPattern::GreaterThan, "5").matches_score(5));
        assert!(!rule(BlockAttribute::Score, BlockPattern::LessThan, "abc").matches_score(-1));
    }

    #[test]
    fn crud_roundtrip_and_describe() {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "jhentai-blockcrud-{}-{}.sqlite",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        let conn = crate::db::open(&path).unwrap();
        let rule = BlockRule {
            group_id: "g".into(),
            target: BlockTarget::Gallery,
            attribute: BlockAttribute::Title,
            pattern: BlockPattern::Contain,
            expression: "spam".into(),
        };
        let id = insert_rule(&conn, &rule).unwrap();
        let all = all_rules(&conn).unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].id, id);
        assert_eq!(all[0].rule, rule, "枚举码往返应一致");
        assert_eq!(describe(&rule), "title contain spam");

        delete_rule(&conn, id).unwrap();
        assert!(all_rules(&conn).unwrap().is_empty());
        drop(conn);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn loads_rules_from_db() {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "jhentai-block-{}-{}.sqlite",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        let conn = crate::db::open(&path).unwrap();
        conn.execute(
            "INSERT INTO block_rule (group_id, target, attribute, pattern, expression) VALUES ('g', 0, 0, 50, 'spam')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO block_rule (group_id, target, attribute, pattern, expression) VALUES ('g', 99, 99, 99, 'x')",
            [],
        )
        .unwrap();

        let set = load_rules(&conn).unwrap();
        assert_eq!(set.rules.len(), 1, "未知枚举码的行应被丢弃");
        assert_eq!(set.rules[0].pattern, BlockPattern::Contain);
        drop(conn);
        let _ = std::fs::remove_file(path);
    }
}
