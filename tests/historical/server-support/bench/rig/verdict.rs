/// 一轮的判决
pub(super) enum Verdict {
    Now,
    /// 问时还没收，**等到收尾事件**后复探确认
    Waited,
    Late,
    Lost,
}
