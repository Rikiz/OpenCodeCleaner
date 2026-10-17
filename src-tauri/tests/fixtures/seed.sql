-- Minimal opencode.db schema fixture (subset, FK cascades enabled)
CREATE TABLE `project` (
  `id` text PRIMARY KEY,
  `worktree` text NOT NULL,
  `vcs` text,
  `name` text,
  `icon_url` text,
  `icon_color` text,
  `time_created` integer NOT NULL,
  `time_updated` integer NOT NULL,
  `time_initialized` integer,
  `sandboxes` text NOT NULL
);

CREATE TABLE `session` (
  `id` text PRIMARY KEY,
  `project_id` text NOT NULL,
  `parent_id` text,
  `slug` text NOT NULL,
  `directory` text NOT NULL,
  `title` text NOT NULL,
  `version` text NOT NULL,
  `share_url` text,
  `time_created` integer NOT NULL,
  `time_updated` integer NOT NULL,
  `time_compacting` integer,
  `time_archived` integer,
  `cost` real DEFAULT 0 NOT NULL,
  `agent` text,
  `model` text,
  FOREIGN KEY (`project_id`) REFERENCES `project`(`id`) ON DELETE CASCADE
);

CREATE TABLE `message` (
  `id` text PRIMARY KEY,
  `session_id` text NOT NULL,
  `time_created` integer NOT NULL,
  `time_updated` integer NOT NULL,
  `data` text NOT NULL,
  FOREIGN KEY (`session_id`) REFERENCES `session`(`id`) ON DELETE CASCADE
);

CREATE TABLE `part` (
  `id` text PRIMARY KEY,
  `message_id` text NOT NULL,
  `session_id` text NOT NULL,
  `time_created` integer NOT NULL,
  `time_updated` integer NOT NULL,
  `data` text NOT NULL,
  FOREIGN KEY (`message_id`) REFERENCES `message`(`id`) ON DELETE CASCADE
);
CREATE INDEX `part_session_idx` ON `part` (`session_id`);

CREATE TABLE `todo` (
  `session_id` text NOT NULL,
  `content` text NOT NULL,
  `status` text NOT NULL,
  `position` integer NOT NULL,
  `time_created` integer NOT NULL,
  `time_updated` integer NOT NULL,
  PRIMARY KEY(`session_id`, `position`),
  FOREIGN KEY (`session_id`) REFERENCES `session`(`id`) ON DELETE CASCADE
);

CREATE TABLE `session_share` (
  `session_id` text PRIMARY KEY,
  `id` text NOT NULL,
  `secret` text NOT NULL,
  `url` text NOT NULL,
  `time_created` integer NOT NULL,
  `time_updated` integer NOT NULL,
  FOREIGN KEY (`session_id`) REFERENCES `session`(`id`) ON DELETE CASCADE
);

CREATE TABLE `event_sequence` (
  `aggregate_id` text PRIMARY KEY,
  `seq` integer NOT NULL
);
CREATE TABLE `event` (
  `id` text PRIMARY KEY,
  `aggregate_id` text NOT NULL,
  `seq` integer NOT NULL,
  `type` text NOT NULL,
  `data` text NOT NULL,
  FOREIGN KEY (`aggregate_id`) REFERENCES `event_sequence`(`aggregate_id`) ON DELETE CASCADE
);

-- seed data: 1 project, 3 sessions with distinct sizes
INSERT INTO project (id, worktree, name, time_created, time_updated, sandboxes)
VALUES ('prj_1', '/tmp/proj', 'demo-project', 0, 0, '[]');

INSERT INTO session (id, project_id, slug, directory, title, version, time_created, time_updated, cost, agent, model)
VALUES
  ('ses_a', 'prj_1', 'a', '/tmp/proj', 'Alpha session', '1.0', 1000, 11000, 0.5, 'build', '{"id":"claude","providerID":"anthropic"}'),
  ('ses_b', 'prj_1', 'b', '/tmp/proj', 'Beta session', '1.0', 2000, 12000, 0.1, 'build', '{"id":"gpt-4o","providerID":"openai"}'),
  ('ses_c', 'prj_1', 'c', '/tmp/proj', 'Gamma session', '1.0', 3000, 13000, 0.0, 'plan', NULL);

INSERT INTO message (id, session_id, time_created, time_updated, data) VALUES
  ('msg_a1','ses_a',1000,1000,'{}'),
  ('msg_b1','ses_b',2000,2000,'{}'),
  ('msg_c1','ses_c',3000,3000,'{}');

-- parts: ses_a biggest, ses_c tiny
INSERT INTO part (id, message_id, session_id, time_created, time_updated, data) VALUES
  ('p_a1','msg_a1','ses_a',1000,1000, json('{"type":"tool","tool":"webfetch","state":{"output":"yyyy text"}}')),
  ('p_a2','msg_a1','ses_a',1000,1000, json('{"type":"text","text":"xxxx"}')),
  ('p_b1','msg_b1','ses_b',2000,2000, json('{"type":"tool","tool":"bash","state":{"output":"z"}}')),
  ('p_c1','msg_c1','ses_c',3000,3000, json('{"type":"text","text":"u"}'));

-- event_sequence keyed by session id (no FK — must be cleaned explicitly)
INSERT INTO event_sequence (aggregate_id, seq) VALUES ('ses_a', 1), ('ses_b', 1), ('ses_c', 1);
INSERT INTO event (id, aggregate_id, seq, type, data) VALUES
  ('e1','ses_a',1,'x','{}'),
  ('e2','ses_b',1,'x','{}'),
  ('e3','ses_c',1,'x','{}');
