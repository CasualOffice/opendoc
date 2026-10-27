# Decisions affecting current feature work

The [ADR register](../08-ADR-REGISTER.md) owns decision text and status. This
page is a reading checklist for work now open in the [single queue](../109-BACKLOG.md),
not another decision register.

| Feature | Decision to review | Work row | Design evidence |
| --- | --- | --- | --- |
| Viewer, editor and native SDK products | ADR-071 | RM-46 | [Three SDK products](../176-THREE-SDK-PRODUCTS.md) |
| MCP and AI-native SDK | ADR-072 | RM-47 | [MCP and AI SDK](../178-MCP-SERVER-AND-AI-NATIVE-SDK.md) |
| Worker threading | ADR-073 | RM-13 | [Measured worker design](../architecture/180-WORKER-THREADING-MEASURED.md) |
| Sparse render backend | ADR-074 | RM-14 | [Measured render backend](../architecture/179-GPU-RENDER-BACKEND-MEASURED.md) |

Before building one of these features, read its ADR in the register, confirm the
owner decision recorded there, and update its queue row. Accepted foundation
ADRs remain in the register and still apply; a feature does not override them
by being listed here.
