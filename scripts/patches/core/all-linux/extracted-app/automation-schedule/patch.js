"use strict";

const {
  patchAutomationScheduleAssets,
} = require("../../../../automation-schedule.js");

module.exports = {
  id: "automation-schedule-multi-time-rrule",
  phase: "extracted-app",
  order: 240,
  ciPolicy: "optional",
  apply: patchAutomationScheduleAssets,
  status: (result, warnings) => ({
    status: result?.changed
      ? "applied"
      : warnings.length > 0
        ? "skipped-optional"
        : result?.matched
          ? "already-applied"
          : "skipped-optional",
    reason: result?.reason ?? warnings[0] ?? null,
  }),
};
