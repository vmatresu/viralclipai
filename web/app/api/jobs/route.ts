import { NextRequest, NextResponse } from 'next/server';
import { v4 as uuidv4 } from 'uuid';
import { adminAuth, adminDb } from '@/lib/server/firebase-admin';
import { redis } from '@/lib/server/redis';

// Input validation types matching Rust implementation
interface CreateJobRequest {
  video_url: string;
  options?: {
    platform?: string;
    style?: string;
    language?: string;
    min_duration?: number;
    max_duration?: number;
  };
}

export async function POST(req: NextRequest) {
  try {
    // 1. Authenticate User
    const authHeader = req.headers.get('Authorization');
    if (!authHeader?.startsWith('Bearer ')) {
      return NextResponse.json({ error: 'Unauthorized' }, { status: 401 });
    }
    const token = authHeader.split('Bearer ')[1];
    const decodedToken = await adminAuth.verifyIdToken(token);
    const userId = decodedToken.uid;

    // 2. Parse & Validate Input
    const body: CreateJobRequest = await req.json();
    if (!body.video_url) {
      return NextResponse.json({ error: 'Missing video_url' }, { status: 400 });
    }

    // 3. Create Job ID & Data
    const jobId = uuidv4();
    const now = new Date().toISOString();
    
    const jobData = {
      id: jobId,
      user_id: userId,
      status: 'queued',
      video_url: body.video_url,
      options: body.options || {},
      created_at: now,
      updated_at: now,
      progress: 0,
      clips_completed: 0,
    };

    // 4. Save to Firestore (Persistence)
    await adminDb.collection('jobs').doc(jobId).set(jobData);

    // 5. Push to Redis Queue (Processing)
    // Using XADD to push to the stream "job_queue"
    // The worker expects the field "job" with the JSON payload
    await redis.xadd('job_queue', '*', {
      job: JSON.stringify(jobData)
    });

    return NextResponse.json({
      job_id: jobId,
      status: 'queued',
      message: 'Job submitted successfully'
    }, { status: 201 });

  } catch (error: any) {
    console.error('Job submission failed:', error);
    return NextResponse.json(
      { error: 'Internal Server Error', details: error.message },
      { status: 500 }
    );
  }
}
