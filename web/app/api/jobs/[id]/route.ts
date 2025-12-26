import { NextRequest, NextResponse } from 'next/server';
import { adminAuth, adminDb } from '@/lib/server/firebase-admin';

export async function GET(
  req: NextRequest,
  { params }: { params: Promise<{ id: string }> }
) {
  try {
    const { id: jobId } = await params;

    // 1. Authenticate User
    const authHeader = req.headers.get('Authorization');
    if (!authHeader?.startsWith('Bearer ')) {
      return NextResponse.json({ error: 'Unauthorized' }, { status: 401 });
    }
    const token = authHeader.split('Bearer ')[1];
    const decodedToken = await adminAuth.verifyIdToken(token);
    const userId = decodedToken.uid;

    // 2. Fetch Job from Firestore
    const jobRef = adminDb.collection('jobs').doc(jobId);
    const jobSnap = await jobRef.get();

    if (!jobSnap.exists) {
      return NextResponse.json({ error: 'Job not found' }, { status: 404 });
    }

    const jobData = jobSnap.data();

    // 3. Verify Ownership
    if (jobData?.user_id !== userId) {
      return NextResponse.json({ error: 'Forbidden' }, { status: 403 });
    }

    // 4. Return Status
    return NextResponse.json({
      job_id: jobId,
      status: jobData.status,
      progress: jobData.progress || 0,
      clips_completed: jobData.clips_completed || 0,
      created_at: jobData.created_at,
      updated_at: jobData.updated_at,
      video_url: jobData.video_url,
      // Include any other relevant fields for the UI
    });

  } catch (error: any) {
    console.error('Get job failed:', error);
    return NextResponse.json(
      { error: 'Internal Server Error', details: error.message },
      { status: 500 }
    );
  }
}
