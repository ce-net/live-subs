#[ce::cell(name = "subs", version = 1, replicas = 3, skill = "Pick an account for a task and say why. Moves tasks, never logins or model traffic.")]
use live::*;
#[op(cap = "subs.read")] async fn windows(cx: Cx, _q: ()) -> Result<Vec<Window>> { live::windows(&cx) }
#[op(cap = "subs.read")] async fn tasks(cx: Cx, q: TaskQuery) -> Result<Vec<Task>> { Ok(live::tasks_for(&cx, q)) }
#[op(cap = "subs.submit", grant = Passkey)] async fn submit(cx: Cx, t: NewTask) -> Result<Task> { live::submit(&cx, t).await }
#[op(cap = "subs.schedule", grant = Passkey)] async fn set_plan(cx: Cx, p: Plan) -> Result<Plan> { cx.twin().put(&p).await?; Ok(p) }
#[op(cap = "subs.schedule")] async fn tick(cx: Cx, _q: ()) -> Result<Vec<Start>> { live::tick(&cx).await }
