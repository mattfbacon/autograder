-- Make sure that any named constraints you add here are also added to `crate::error::constraint_message`.

create table problems (
	id integer primary key,
	name text not null constraint problems_name_not_empty check (name != '') constraint problems_name check (name not regexp '[\x01-\x1f\x80-\x9f]'),
	description text not null constraint problems_description check (description not regexp '[\x01-\x1f\x80-\x9f]'),
	time_limit integer not null,
	visible integer not null,
	tests text not null,
	custom_judger text,
	creation_time integer not null,
	created_by integer references users on delete set null
) strict;

create table submissions (
	id integer primary key,
	code text not null,
	for_problem integer not null references problems on delete cascade,
	submitter integer not null references users on delete cascade,
	language integer not null,
	submission_time integer not null,
	judged_time integer,
	result text
) strict;

create index submissions_problem on submissions(for_problem);
create index submissions_submitter on submissions(submitter);

create table users (
	id integer primary key,
	username text not null constraint users_username_unique unique constraint users_username check (username regexp '^[a-z0-9_]+$'),
	display_name text not null constraint users_display_name_not_empty check (display_name != '') constraint users_display_name check (display_name not regexp '[\x01-\x1f\x80-\x9f]'),
	email text,
	password text not null,
	creation_time integer not null,
	permission_level integer not null,

	password_reset_expiration integer,
	password_reset_key integer,

	remove_email_key integer not null default (random())
) strict;

create table sessions (
	token blob not null primary key,
	user integer not null references users on delete cascade,
	expiration integer not null
) without rowid, strict;
