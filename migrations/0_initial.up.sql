create table problems (
	id integer primary key,
	name text not null,
	description text not null,
	time_limit integer not null,
	memory_limit integer not null,
	visible integer not null,
	tests text not null,
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
	username text unique not null,
	display_name text not null,
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
	user integer unique not null references users on delete cascade
) without rowid, strict;
